Библиотека ORM ([Object-relational mapping](https://en.wikipedia.org/wiki/Object%E2%80%93relational_mapping)).

## Описание

В прикладных задачах часто возникает необходимость хранить данные во внешней базе. Зачастую
такие базы хранят данные в виде таблиц (так устроены все реляционные СУБД). Однако, в программном
коде работать с табличными данными неудобно, намного удобнее работать с нативными для языка
структурами или объектами. Задача ORM-библиотеки - дать прикладному коду возможность работать
с данными как с объектами, абстрагируясь от того, как именно данные будут разложены в таблице.

Продвинутые ORM-библиотеки поддерживают множество разных СУБД в качестве бекенда. В рамках данного
проекта мы ограничимся поддержкой лишь одной СУБД - SQLite3. Однако архитектура библиотеки
поддерживает возможность добавления других бекендов.

### Примеры

Рассмотрим парочку примеров из тестов:

```rust
#[derive(Object)]
struct User {
    name: String,
    picture: Vec<u8>,
    visits: i64,
    balance: f64,
    is_admin: bool,
}
```

Структура User содержит в себе поля всех пяти типов, которые наша библиотека поддерживает.
`#[derive(Object)]` реализует для `User` трейт `Object`, необходимый для работы библиотеки.


В нашей ORM работа с СУБД возможна только в рамках транзакций, которые создаются так:

```rust
// Создать соединение с СУБД.
let mut conn = Connection::open_sqlite_file("/path/to/file").unwrap();
// Создать транзакцию.
let tx = conn.new_transaction().unwrap();
```

Имея транзакцию, мы можем создать в ней объект:

```rust
// Создаём объект в памяти. Пока что этот объект не привязан ни к какой транзакции.
let user = User { /* ... */ };
// Теперь создадим этот объект в СУБД в рамках транзакции.
let tx_user = tx.create(user).unwrap();
```

Метод `create` возвращает значение типа `Tx<'a, User>`. Семантически это объект типа `User`, который
существует в рамках транзакции. Объект привязан к транзакции лайфтаймом `'a`, т.е. не может пережить
свою транзакцию.

У каждого существующего в рамках транзакции объекта есть идентификатор:

```rust
let user_id = tx_user.id();
```

В нашей ORM идентификаторы целочисленные.

Другой способ получить объект в рамках транзакции - это прочитать его из базы:

```rust
let tx_user = tx.get::<User>(user_id);
```

Чтобы читать или писать поля принадлежащего транзакции объекта, нужно использовать методы
`.borrow()` и `.borrow_mut()`:

```rust
println!("User name: {}", tx_user.borrow().name);
*tx_user.borrow_mut().visits += 1;
```

Можно заселектить один и тот же объект из базы дважды. Тогда объекты `Tx<...>`, которые транзакция
вернёт, будут ссылаться на один и тот же объект в памяти:

```rust
let tx_user = tx.get::<User>(user_id);
let tx_user_2 = tx.get::<User>(user_id);
*tx_user.borrow_mut().balance = 250;
assert_eq!(tx_user_2.borrow().balance, 250);
```

Если позвать `.borrow_mut()` на объект, уже имеющий активные borrows, произойдёт паника. Точно также
произойдёт паника, если позвать `.borrow()` на объект, имеющий активное mutable borrow.

Также, имея принадлежащий транзакции объект, можно его удалить:

```rust
tx_user.delete();
```

Если объект имеет активные borrows, произойдёт паника. Также к панике приведёт попытка позвать
`.borrow()` или `.borrow_mut()` на объект, который удалён (например, через `tx_user_2` в примере
выше).

Чтобы применить все изменения в рамках транзакции, необходимо завершить её вызовом `tx.commit()`.
Вызов `tx.rollback()`, наоборот, завершит транзакцию откатом всех изменений.

### Имена таблиц и колонок

По-умолчанию, таблица в СУБД называется одноимённо с типом объекта, а колонки - одноимённо с полями
объекта. Однако, имена таблиц и колонок можно менять атрибутами `table_name` и `column_name` на
структуре, например:

```rust
#[derive(Object)]
#[table_name("order_table")]
struct Order {
    #[column_name("IsTall")]
    is_tall: bool,
}
```

## Детали Реализации

### Трейт Object

Трейт `Object` объявляется в `src/object.rs`.

`Object` содержит в себе:
* `SCHEMA`: название типа объекта, название таблицы, список полей объекта (для каждого поля - его имя,
название колонки и тип).
* `as_row()` - представлние объекта в виде строчки в таблице.
* `from_row()`- создать экземпляр объекта из строчки в таблице.

Трейт `Store` - это object safe обертка над `Object`, чтобы иметь возможность использовать `dyn Store` для хранения объектов.

### Работа с SQL

Вся работа с SQL инкапсулируется трейтом `StorageTransaction`, объявленным в `src/storage.rs`.
Задача `StorageTransaction` - предоставить нашей библиотеке интерфейс работы со стораджем,
абстрагированный от конкретной библиотеки.

Для работы с SQLite3 мы будем использовать библиотеку `rusqlite`.
Трейт `StorageTransaction` реализован для `rusqlite::Transaction`. Для поддержки любого другого бэкенда библиотекой, достаточно реализовать данный трейт.

### Транзакция и кеш объектов

Каждый объект, инстанциированный в рамках транзакции ORM (не путать с транзакцией rusqlite), храниться в кеше объектов этой транзакции.
При коммите транзакции мы проходимся по кешу объектов, проверяя, какие объекты были изменены,
и применить эти изменения к нижележащей `StorageTransaction` (через метод `.update_row()`).
Те объекты, которые были удалены, удаляются (`.remove_row()`).

### Обработка ошибок

Ошибки объявлены в `src/error.rs`. В рамках проекта выделены пять разновидностей ошибок:
* `NotFound` - запрошенный объект не найден.
* `UnexpectedType` - в одной из колонок получен не тот тип, который ожидался объектом.
* `MissingColumn` - какая-то из ожидаемых колонок отсутствует в таблице.
* `LockConflict` - база заблокирована конкурентной транзакцией (SQLite3 при работе с таблицей лочит
её целиком)
* `Storage` - любая другая ошибка нижележащего стораджа.

Мапинг из ошибок rusqlite в ошибки нашей библиотеки следующий:
* Ошибка `rusqlite::Error::QueryReturnedNoRows` - это `NotFound`.
* Ошибка `rusqlite::Error::InvalidColumnType` - это `UnexpectedType`.
* Ошибка `rusqlite::Error::SqliteFailure` c кодом `rusqlite::ErrorCode::DatabaseBusy` - это `LockConflict`.
* Ошибка `rusqlite::Error::SqliteFailire`, содержащая текст "no such column:" или "has no column named" -
это `MissingColumn`.
* Всё остальное - это `StorageError`.
