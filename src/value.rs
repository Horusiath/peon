use std::io::Write;

macro_rules! impl_from_number {
    ($($t:ty),+) => {
        $(
            impl From<$t> for Value {
                fn from(value: $t) -> Self {
                    Value::Int(value as i128)
                }
            }
        )+
    };
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Value {
    Int(i128),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
}

impl Value {
    pub fn write_to<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        match self {
            Value::Int(value) => Self::write_varint(value, writer),
            Value::Float(value) => {}
            Value::String(value) => {}
            Value::Bytes(value) => {}
        }
    }

    fn write_varint<W: Write>(value: &i128, writer: &mut W) -> std::io::Result<()> {
        todo!()
    }
}

impl_from_number!(
    u8, u16, u32, u64, u128, i8, i16, i32, i64, i128, usize, isize
);

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Int(if value { 1 } else { 0 })
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Float(value)
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Value::Float(value as f64)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::String(value)
    }
}

impl<'a> From<&'a str> for Value {
    fn from(value: &'a str) -> Self {
        Value::String(value.to_string())
    }
}

impl From<Vec<u8>> for Value {
    fn from(value: Vec<u8>) -> Self {
        Value::Bytes(value)
    }
}

impl<'a> From<&'a [u8]> for Value {
    fn from(value: &'a [u8]) -> Self {
        Value::Bytes(value.into())
    }
}

#[cfg(feature = "serde_json")]
impl From<Value> for serde_json::Value {
    fn from(value: Value) -> Self {
        match value {
            Value::Int(value) => {
                serde_json::Value::Number(serde_json::Number::from_i128(value).unwrap())
            }
            Value::Float(value) => {
                serde_json::Value::Number(serde_json::Number::from_f64(value).unwrap())
            }
            Value::String(value) => serde_json::Value::String(value),
            Value::Bytes(value) => serde_json::Value::String(simple_base64::encode(&value)),
        }
    }
}
