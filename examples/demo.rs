use integer_encoding::VarInt;
use peon::json::Flatten;
use serde_json::Value;
use std::time::Instant;

fn main() {
    let path = "assets/complex.json";
    let content = std::fs::read_to_string(path).expect("Failed to read the file");
    println!("original content size: {} bytes", content.len());

    let json: Value = serde_json::from_str(&content).expect("Failed to serialize to JSON");

    let flattened: Vec<_> = json.flatten().into_iter().collect();
    let entry_count = flattened.len();
    let mut buf = Vec::new();
    let mut value_buf = Vec::new();
    let mut encoder = peon::PrefixEncoder::new(&mut buf);
    let start = Instant::now();

    for (path, value) in flattened {
        value_buf.clear();
        if !encode_scalar(&value, &mut value_buf) {
            continue; // Skip null values
        }

        encoder
            .write_next(path.as_ref(), &value_buf)
            .expect("Failed to write next");
    }

    let end = Instant::now();
    println!(
        "written {} entries in {:?} (size: {} bytes)",
        entry_count,
        end.duration_since(start),
        buf.len()
    );

    let mut decoder = peon::PrefixDecoder::new(std::io::Cursor::new(buf));
    let start = Instant::now();
    let mut i = 0;
    while let Some((path, value)) = decoder.read_next().expect("Failed to read next") {
        //println!("{} => {}", path, decode_scalar(value).unwrap());
        i += 1;
    }
    let end = Instant::now();
    println!("read {} entries in {:?}", i, end.duration_since(start));
}

const TAG_BOOL_FALSE: u8 = 0b0000_0000;
const TAG_BOOL_TRUE: u8 = 0b0000_0001;
const TAG_BOOL_FLOAT: u8 = 0b0000_0010;
const TAG_BOOL_INTEGER: u8 = 0b0000_0011;
const TAG_BOOL_STRING: u8 = 0b0000_0100;

fn encode_scalar(value: &Value, buf: &mut Vec<u8>) -> bool {
    match value {
        Value::Null => return false, // Null values are ignored
        Value::Bool(b) => buf.push(if *b { TAG_BOOL_TRUE } else { TAG_BOOL_FALSE }),
        Value::Number(num) => {
            if let Some(i) = num.as_i64() {
                let mut b = [0; 10];
                let size = i.encode_var(&mut b);
                buf.push(TAG_BOOL_INTEGER);
                buf.extend_from_slice(&b[..size]);
            } else if let Some(f) = num.as_f64() {
                buf.push(TAG_BOOL_FLOAT);
                buf.extend_from_slice(&f.to_le_bytes());
            } else {
                panic!("Unsupported number type");
            }
        }
        Value::String(s) => {
            buf.push(TAG_BOOL_STRING);
            buf.extend_from_slice(s.as_bytes())
        }
        _ => panic!("arrays and maps should be flattened"),
    }
    true
}

fn decode_scalar(buf: &[u8]) -> Option<Value> {
    if buf.is_empty() {
        return None;
    }

    match buf[0] {
        TAG_BOOL_FALSE => Some(Value::Bool(false)),
        TAG_BOOL_TRUE => Some(Value::Bool(true)),
        TAG_BOOL_FLOAT => {
            if buf.len() < 9 {
                return None; // Not enough bytes for f64
            }
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&buf[1..9]);
            Some(Value::Number(
                serde_json::Number::from_f64(f64::from_le_bytes(bytes)).unwrap(),
            ))
        }
        TAG_BOOL_INTEGER => {
            let (i, _size) = i64::decode_var(&buf[1..]).unwrap();
            Some(Value::Number(serde_json::Number::from(i)))
        }
        TAG_BOOL_STRING => {
            let s = String::from_utf8(buf[1..].to_vec()).ok()?;
            Some(Value::String(s))
        }
        _ => None,
    }
}
