#![forbid(unsafe_code)]
use crate::{
    data::ObjectId,
    error::{Error, Result},
    object::{Object, Store},
    storage::StorageTransaction,
};
use std::{
    any::{Any, TypeId},
    cell::{Cell, Ref, RefCell, RefMut},
    collections::{hash_map::Entry, HashMap},
    marker::PhantomData,
    rc::Rc,
};

////////////////////////////////////////////////////////////////////////////////
pub struct Transaction<'a> {
    inner: Box<dyn StorageTransaction + 'a>,
    cache: RefCell<HashMap<CacheKey, CacheValue>>,
}

impl<'a> Transaction<'a> {
    pub(crate) fn new(inner: Box<dyn StorageTransaction + 'a>) -> Self {
        Self {
            inner,
            cache: RefCell::new(HashMap::new()),
        }
    }

    fn ensure_table<T: Object>(&self) -> Result<()> {
        if self.inner.table_exists(T::SCHEMA.table_name)? {
            return Ok(());
        }

        self.inner.create_table(&T::SCHEMA)
    }

    pub fn create<T: Object>(&self, src_obj: T) -> Result<Tx<'_, T>> {
        self.ensure_table::<T>()?;
        let id = self.inner.insert_row(&T::SCHEMA, &src_obj.as_row())?;

        let obj = Rc::new(RefCell::new(src_obj));
        let state = Rc::new(Cell::new(ObjectState::Clean));

        self.cache.borrow_mut().insert(
            (TypeId::of::<T>(), id),
            CacheValue {
                state: state.clone(),
                stored: obj.clone(),
            },
        );

        Ok(Tx {
            obj,
            state,
            id,

            _lifetime: PhantomData,
            _refers_object: PhantomData,
        })
    }

    pub fn get<T: Object>(&self, id: ObjectId) -> Result<Tx<'_, T>> {
        self.ensure_table::<T>()?;
        let mut borrowed_cache = self.cache.borrow_mut();
        let cached = match borrowed_cache.entry((TypeId::of::<T>(), id)) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let obj = T::from_row(self.inner.select_row(id, &T::SCHEMA)?);

                entry.insert(CacheValue {
                    state: Rc::new(Cell::new(ObjectState::Clean)),
                    stored: Rc::new(RefCell::new(obj)),
                })
            }
        };

        if cached.state.get() == ObjectState::Removed {
            return Err(Error::not_found(id, T::SCHEMA.type_name));
        }

        Ok(Tx {
            state: cached.state.clone(),
            obj: cached.stored.clone(),
            id,

            _lifetime: PhantomData,
            _refers_object: PhantomData,
        })
    }

    fn try_apply(&self) -> Result<()> {
        for ((_, id), cached) in self.cache.borrow().iter() {
            let obj = (*cached.stored).borrow();
            match cached.state.get() {
                ObjectState::Modified => {
                    self.inner.update_row(*id, obj.schema(), &obj.as_row())?;
                }
                ObjectState::Removed => {
                    self.inner.delete_row(*id, obj.schema())?;
                }
                ObjectState::Clean => (),
            }
        }

        Ok(())
    }

    pub fn commit(self) -> Result<()> {
        self.try_apply()?;
        self.inner.commit()
    }

    pub fn rollback(self) -> Result<()> {
        self.inner.rollback()
    }
}

type CacheKey = (TypeId, ObjectId);

struct CacheValue {
    state: Rc<Cell<ObjectState>>,
    stored: Rc<RefCell<dyn Store>>,
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ObjectState {
    Clean,
    Modified,
    Removed,
}

#[derive(Clone)]
pub struct Tx<'a, T> {
    state: Rc<Cell<ObjectState>>,
    obj: Rc<RefCell<dyn Store>>,
    id: ObjectId,

    _lifetime: PhantomData<&'a Transaction<'a>>,
    _refers_object: PhantomData<Rc<RefCell<T>>>,
}

impl<'a, T: Any> Tx<'a, T> {
    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn state(&self) -> ObjectState {
        self.state.get()
    }

    pub fn borrow(&self) -> Ref<'_, T> {
        if self.state() == ObjectState::Removed {
            panic!("cannot borrow a removed object");
        }
        Ref::map((*self.obj).borrow(), |stored| {
            stored.as_any().downcast_ref::<T>().unwrap()
        })
    }

    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        if self.state() == ObjectState::Removed {
            panic!("cannot borrow a removed object");
        }
        self.state.set(ObjectState::Modified);
        RefMut::map(self.obj.borrow_mut(), |stored| {
            stored.as_any_mut().downcast_mut::<T>().unwrap()
        })
    }

    pub fn delete(self) {
        match self.obj.try_borrow_mut() {
            Ok(_) => self.state.set(ObjectState::Removed),
            Err(_) => panic!("cannot delete a borrowed object"),
        }
    }
}
