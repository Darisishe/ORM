#![forbid(unsafe_code)]
use crate::{
    data::{DataType, Value},
    error::{Error, ErrorCtx, ErrorWithCtx, Result},
    object::Schema,
    ObjectId,
};
use rusqlite::{params_from_iter, ToSql};
use std::iter;

////////////////////////////////////////////////////////////////////////////////

pub type Row<'a> = Vec<Value<'a>>;
pub type RowSlice<'a> = [Value<'a>];

////////////////////////////////////////////////////////////////////////////////

pub(crate) trait StorageTransaction {
    fn table_exists(&self, table: &str) -> Result<bool>;
    fn create_table(&self, schema: &Schema) -> Result<()>;

    fn insert_row(&self, schema: &Schema, row: &RowSlice) -> Result<ObjectId>;
    fn update_row(&self, id: ObjectId, schema: &Schema, row: &RowSlice) -> Result<()>;
    fn select_row(&self, id: ObjectId, schema: &Schema) -> Result<Row<'static>>;
    fn delete_row(&self, id: ObjectId, schema: &Schema) -> Result<()>;

    fn commit(&self) -> Result<()>;
    fn rollback(&self) -> Result<()>;
}

////////////////////////////////////////////////////////////////////////////////

impl<'a> StorageTransaction for rusqlite::Transaction<'a> {
    fn table_exists(&self, table: &str) -> Result<bool> {
        let mut stmt = self.prepare("SELECT 1 FROM sqlite_master WHERE name = ?")?;
        Ok(stmt.exists([table])?)
    }

    fn create_table(&self, schema: &Schema) -> Result<()> {
        let columns = iter::once("id INTEGER PRIMARY KEY AUTOINCREMENT".to_string())
            .chain(schema.fields.iter().map(|field| {
                format!(
                    "{} {}",
                    field.column_name,
                    data_type_as_sqlite(field.column_type)
                )
            }))
            .collect::<Vec<_>>()
            .join(", ");

        let sql = format!("CREATE TABLE {} ({})", schema.table_name, columns);

        self.execute(&sql, [])?;
        Ok(())
    }

    fn insert_row(&self, schema: &Schema, row: &RowSlice) -> Result<ObjectId> {
        let columns = schema.column_names().collect::<Vec<_>>().join(", ");
        let sql = if !schema.fields.is_empty() {
            format!(
                "INSERT INTO {} ({}) VALUES({})",
                schema.table_name,
                columns,
                repeat_with_comma("?", schema.fields.len())
            )
        } else {
            format!("INSERT INTO {} DEFAULT VALUES", schema.table_name)
        };

        let ctx_with_schema = ErrorCtx {
            schema: Some(schema),
            ..Default::default()
        };

        let mut stmt = self
            .prepare(&sql)
            .map_err(|err| Error::from(ErrorWithCtx::new(err, ctx_with_schema.clone())))?;

        match stmt.insert(params_from_iter(row.iter())) {
            Ok(id) => Ok(ObjectId(id)),
            Err(err) => Err(Error::from(ErrorWithCtx::new(err, ctx_with_schema))),
        }
    }

    fn update_row(&self, id: ObjectId, schema: &Schema, row: &RowSlice) -> Result<()> {
        let columns = schema
            .column_names()
            .map(|col| format!("{} = ?", col))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("UPDATE {} SET {} WHERE id = ?", schema.table_name, columns);
        let params = row_to_sql(row).chain(iter::once(&id.0 as &dyn ToSql));

        match self.execute(&sql, params_from_iter(params)) {
            Ok(_) => Ok(()),
            Err(error) => Err(Error::from(ErrorWithCtx::new(
                error,
                ErrorCtx {
                    object_id: Some(id),
                    schema: Some(schema),
                },
            ))),
        }
    }

    fn select_row(&self, id: ObjectId, schema: &Schema) -> Result<Row<'static>> {
        let columns = if !schema.fields.is_empty() {
            schema.column_names().collect::<Vec<_>>().join(", ")
        } else {
            "*".to_string()
        };
        let sql = format!("SELECT {} FROM {} WHERE id = ?", columns, schema.table_name);

        let ctx = ErrorCtx {
            schema: Some(schema),
            object_id: Some(id),
        };

        let mut stmt = self
            .prepare(&sql)
            .map_err(|error| Error::from(ErrorWithCtx::new(error, ctx.clone())))?;

        let mut rows = stmt
            .query([id.0])
            .map_err(|error| Error::from(ErrorWithCtx::new(error, ctx.clone())))?;

        let row = match rows.next() {
            Ok(Some(r)) => Ok(r),
            Ok(None) => Err(Error::not_found(id, schema.type_name)),
            Err(error) => Err(Error::from(ErrorWithCtx::new(error, ctx.clone()))),
        }?;

        let mut res = Row::with_capacity(schema.fields.len());
        for field in schema.fields {
            let val = extract_value_from_row(field.column_type, row, field.column_name)
                .map_err(|error| Error::from(ErrorWithCtx::new(error, ctx.clone())))?;

            res.push(val);
        }

        Ok(res)
    }

    fn delete_row(&self, id: ObjectId, schema: &Schema) -> Result<()> {
        let sql = format!("DELETE FROM {} WHERE id = ?", schema.table_name);

        match self.execute(&sql, [id.0]) {
            Ok(_) => Ok(()),
            Err(error) => Err(Error::from(ErrorWithCtx::new(
                error,
                ErrorCtx {
                    object_id: Some(id),
                    schema: Some(schema),
                },
            ))),
        }
    }

    fn commit(&self) -> Result<()> {
        self.execute("COMMIT", [])?;
        Ok(())
    }

    fn rollback(&self) -> Result<()> {
        self.execute("ROLLBACK", [])?;
        Ok(())
    }
}

////////////////////////////////////////////////////////////////////////////////

fn data_type_as_sqlite(data_type: DataType) -> &'static str {
    match data_type {
        DataType::String => "TEXT",
        DataType::Bytes => "BLOB",
        DataType::Int64 => "BIGINT",
        DataType::Float64 => "REAL",
        DataType::Bool => "TINYINT",
    }
}

impl<'a> ToSql for Value<'a> {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self {
            Value::String(string) => string.to_sql(),
            Value::Bytes(bytes) => bytes.to_sql(),
            Value::Int64(x) => x.to_sql(),
            Value::Float64(x) => x.to_sql(),
            Value::Bool(x) => x.to_sql(),
        }
    }
}

fn repeat_with_comma(pattern: &str, count: usize) -> String {
    vec![pattern; count].join(", ")
}

fn row_to_sql<'a>(row: &'a RowSlice<'a>) -> impl Iterator<Item = &'a dyn ToSql> {
    row.iter().map(|val| val as &dyn ToSql)
}

fn extract_value_from_row(
    column_type: DataType,
    row: &rusqlite::Row,
    column_name: &str,
) -> rusqlite::Result<Value<'static>> {
    Ok(match column_type {
        DataType::String => Value::String(row.get::<_, String>(column_name)?.into()),
        DataType::Bytes => Value::Bytes(row.get::<_, Vec<u8>>(column_name)?.into()),
        DataType::Int64 => Value::Int64(row.get(column_name)?),
        DataType::Float64 => Value::Float64(row.get(column_name)?),
        DataType::Bool => Value::Bool(row.get(column_name)?),
    })
}
