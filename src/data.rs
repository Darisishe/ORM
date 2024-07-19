#![forbid(unsafe_code)]
use std::{borrow::Cow, fmt};

////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct ObjectId(pub i64);

impl From<i64> for ObjectId {
    fn from(value: i64) -> Self {
        ObjectId(value)
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ObjectId {
    pub fn into_i64(&self) -> i64 {
        self.0
    }
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    String,
    Bytes,
    Int64,
    Float64,
    Bool,
}

////////////////////////////////////////////////////////////////////////////////

pub enum Value<'a> {
    String(Cow<'a, str>),
    Bytes(Cow<'a, [u8]>),
    Int64(i64),
    Float64(f64),
    Bool(bool),
}

////////////////////////////////////////////////////////////////////////////////

pub trait AsDataType {
    const DATA_TYPE: DataType;

    fn as_value(&self) -> Value;
    fn from_value(value: &Value) -> Self;
}

impl AsDataType for String {
    const DATA_TYPE: DataType = DataType::String;

    fn as_value(&self) -> Value {
        Value::String(std::borrow::Cow::from(self))
    }

    fn from_value(value: &Value) -> Self {
        if let Value::String(s) = value {
            s.clone().into_owned()
        } else {
            panic!("not expected type")
        }
    }
}

impl AsDataType for Vec<u8> {
    const DATA_TYPE: DataType = DataType::Bytes;

    fn as_value(&self) -> Value {
        Value::Bytes(std::borrow::Cow::from(self))
    }

    fn from_value(value: &Value) -> Self {
        if let Value::Bytes(b) = value {
            b.clone().into_owned()
        } else {
            panic!("not expected type")
        }
    }
}

impl AsDataType for i64 {
    const DATA_TYPE: DataType = DataType::Int64;

    fn as_value(&self) -> Value {
        Value::Int64(*self)
    }

    fn from_value(value: &Value) -> Self {
        if let Value::Int64(x) = value {
            *x
        } else {
            panic!("not expected type")
        }
    }
}

impl AsDataType for f64 {
    const DATA_TYPE: DataType = DataType::Float64;

    fn as_value(&self) -> Value {
        Value::Float64(*self)
    }

    fn from_value(value: &Value) -> Self {
        if let Value::Float64(x) = value {
            *x
        } else {
            panic!("not expected type")
        }
    }
}

impl AsDataType for bool {
    const DATA_TYPE: DataType = DataType::Bool;

    fn as_value(&self) -> Value {
        Value::Bool(*self)
    }

    fn from_value(value: &Value) -> Self {
        if let Value::Bool(x) = value {
            *x
        } else {
            panic!("not expected type")
        }
    }
}
