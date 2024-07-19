#![forbid(unsafe_code)]
use crate::{data::DataType, storage::Row};
use std::any::Any;

////////////////////////////////////////////////////////////////////////////////

pub trait Object: Any + Sized {
    fn as_row(&self) -> Row;
    fn from_row(row: Row) -> Self;

    const SCHEMA: Schema;
}

////////////////////////////////////////////////////////////////////////////////

pub trait Store: Any {
    fn as_row(&self) -> Row;
    fn schema(&self) -> &Schema;

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Object> Store for T {
    fn as_row(&self) -> Row {
        self.as_row()
    }

    fn schema(&self) -> &Schema {
        &Self::SCHEMA
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

////////////////////////////////////////////////////////////////////////////////

pub struct Schema {
    pub type_name: &'static str,
    pub table_name: &'static str,

    // static, because list is created at compile-time by derive macro
    pub fields: &'static [Field],
}

impl Schema {
    pub fn column_types(&self) -> impl Iterator<Item = DataType> {
        self.fields.iter().map(|field| field.column_type)
    }

    pub fn column_names(&self) -> impl Iterator<Item = &'static str> {
        self.fields.iter().map(|field| field.column_name)
    }
}

#[derive(Clone)]
pub struct Field {
    pub attr_name: &'static str,
    pub column_name: &'static str,
    pub column_type: DataType,
}
