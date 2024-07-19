#![forbid(unsafe_code)]
use crate::{
    data::DataType,
    object::{Field, Schema},
    ObjectId,
};
use thiserror::Error;

////////////////////////////////////////////////////////////////////////////////

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    NotFound(Box<NotFoundError>),
    #[error(transparent)]
    UnexpectedType(Box<UnexpectedTypeError>),
    #[error(transparent)]
    MissingColumn(Box<MissingColumnError>),
    #[error("database is locked")]
    LockConflict,
    #[error("storage error: {0}")]
    Storage(#[source] Box<dyn std::error::Error>),
}

impl<'a> From<ErrorWithCtx<'a, rusqlite::Error>> for Error {
    fn from(err: ErrorWithCtx<'a, rusqlite::Error>) -> Self {
        let context = err.ctx;

        match err.err {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code: rusqlite::ErrorCode::DatabaseBusy,
                    ..
                },
                _,
            ) => Error::LockConflict,

            rusqlite::Error::SqliteFailure(_, Some(text))
                if text.contains("no such column:") || text.contains("has no column named") =>
            {
                let column_name = match text.find("no such column: ") {
                    Some(ind) => text[ind..].strip_prefix("no such column: ").unwrap(),
                    None => {
                        let ind = text.find("has no column named ").unwrap();
                        text[ind..].strip_prefix("has no column named ").unwrap()
                    }
                };
                dbg!(&column_name);

                let schema = context
                    .schema
                    .expect("Schema should be provided to context");
                let field = get_field_by_name(schema, column_name);

                Error::MissingColumn(Box::new({
                    MissingColumnError {
                        type_name: schema.type_name,
                        attr_name: field.attr_name,
                        table_name: schema.table_name,
                        column_name: field.column_name,
                    }
                }))
            }

            rusqlite::Error::QueryReturnedNoRows => Error::NotFound(Box::new(NotFoundError {
                object_id: context
                    .object_id
                    .expect("object_id should be provided to context"),
                type_name: context
                    .schema
                    .expect("Schema should be provided to context")
                    .type_name,
            })),

            rusqlite::Error::InvalidColumnType(_, column_name, got_type) => {
                let schema = context
                    .schema
                    .expect("Schema should be provided to context");
                let field = get_field_by_name(schema, &column_name);

                Error::UnexpectedType(Box::new(UnexpectedTypeError {
                    type_name: schema.type_name,
                    attr_name: field.attr_name,
                    table_name: schema.table_name,
                    column_name: field.column_name,
                    expected_type: field.column_type,
                    got_type: got_type.to_string(),
                }))
            }

            _ => Error::Storage(Box::new(err.err)),
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Self::from(ErrorWithCtx::new(err, ErrorCtx::default()))
    }
}

impl Error {
    pub(crate) fn not_found(object_id: ObjectId, type_name: &'static str) -> Error {
        Error::NotFound(Box::new(NotFoundError {
            object_id,
            type_name,
        }))
    }
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Error, Debug)]
#[error("object is not found: type '{type_name}', id {object_id}")]
pub struct NotFoundError {
    pub object_id: ObjectId,
    pub type_name: &'static str,
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Error, Debug)]
#[error(
    "invalid type for {type_name}::{attr_name}: expected equivalent of {expected_type:?}, \
    got {got_type} (table: {table_name}, column: {column_name})"
)]
pub struct UnexpectedTypeError {
    pub type_name: &'static str,
    pub attr_name: &'static str,
    pub table_name: &'static str,
    pub column_name: &'static str,
    pub expected_type: DataType,
    pub got_type: String,
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Error, Debug)]
#[error(
    "missing a column for {type_name}::{attr_name} \
    (table: {table_name}, column: {column_name})"
)]
pub struct MissingColumnError {
    pub type_name: &'static str,
    pub attr_name: &'static str,
    pub table_name: &'static str,
    pub column_name: &'static str,
}

pub(crate) struct ErrorWithCtx<'a, E> {
    err: E,
    ctx: ErrorCtx<'a>,
}

impl<E> ErrorWithCtx<'_, E> {
    pub fn new(err: E, ctx: ErrorCtx) -> ErrorWithCtx<E> {
        ErrorWithCtx { err, ctx }
    }
}

#[derive(Default, Clone)]
pub(crate) struct ErrorCtx<'a> {
    pub schema: Option<&'a Schema>,
    pub object_id: Option<ObjectId>,
}

fn get_field_by_name(schema: &Schema, column_name: &str) -> Field {
    for field in schema.fields.iter() {
        if field.column_name == column_name {
            return field.clone();
        }
    }

    Field {
        attr_name: "id",
        column_name: "id",
        column_type: DataType::Int64,
    }
}

////////////////////////////////////////////////////////////////////////////////

pub type Result<T> = std::result::Result<T, Error>;
