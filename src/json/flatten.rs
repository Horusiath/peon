use crate::json::{TAG_BOOL_FALSE, TAG_BOOL_TRUE, TAG_FLOAT, TAG_INTEGER, TAG_NULL, TAG_STRING};
use crate::{PathBuf, size_hint};
use smallvec::{SmallVec, smallvec};

pub trait Flatten {
    type Value;
    type Iter: IntoIterator<Item = (PathBuf<Vec<u8>>, Self::Value)>;
    /// Flattens the JSON structure into a single-level map.
    ///
    /// The keys in the map are JSON paths, and the values are the corresponding JSON values.
    fn flatten(self, chunk_size: usize) -> Self::Iter;
}

impl Flatten for serde_json::Value {
    type Value = super::Value;
    type Iter = Vec<(PathBuf<Vec<u8>>, Self::Value)>;

    fn flatten(self, chunk_size: usize) -> Self::Iter {
        let mut acc = Vec::new();
        let mut path = PathBuf::new(Vec::new());

        flatten_inner(chunk_size, &self, &mut path, &mut acc);

        acc
    }
}

fn flatten_inner(
    chunk_size: usize,
    value: &serde_json::Value,
    path_buf: &mut PathBuf<Vec<u8>>,
    acc: &mut Vec<(PathBuf<Vec<u8>>, super::Value)>,
) {
    match value {
        serde_json::Value::Null => {
            acc.push((path_buf.clone(), smallvec![TAG_NULL]));
        }
        serde_json::Value::Array(array) => {
            for (index, item) in array.iter().enumerate() {
                let mut path_buf = path_buf.clone();
                // Push the current index to the path
                path_buf.push_index(index as u64).unwrap();
                // Recursively flatten the item
                flatten_inner(chunk_size, item, &mut path_buf, acc);
            }
        }
        serde_json::Value::Object(array) => {
            for (key, item) in array.iter() {
                let mut path_buf = path_buf.clone();
                // Push the current key to the path
                path_buf.push_key(key).unwrap();
                // Recursively flatten the item
                flatten_inner(chunk_size, item, &mut path_buf, acc);
            }
        }
        serde_json::Value::String(value) => {
            let bytes = value.as_bytes();
            if bytes.len() <= chunk_size {
                let mut buf = super::Value::with_capacity(chunk_size + 1);
                buf.push(TAG_STRING);
                buf.extend_from_slice(bytes);
                acc.push((path_buf.clone(), buf));
            } else {
                let mut index = 0usize;
                while index < bytes.len() {
                    let mut path_buf = path_buf.clone();
                    // Push the current chunk to the path
                    path_buf.push_index(index as u64).unwrap();
                    path_buf.push_continued().unwrap();
                    let path_len = path_buf.as_bytes().len();
                    let chunk_len = (chunk_size - path_len - 6).min(bytes.len() - index);
                    let chunk = &bytes[index..(index + chunk_len)];
                    // Push the chunked value
                    acc.push((path_buf, SmallVec::from_slice(chunk)));
                    index += chunk_len;
                }
            }
        }
        serde_json::Value::Number(v) => {
            if let Some(v) = value.as_i64() {
                let zigzag = if v < 0 {
                    (v << 1) as u64 - 1
                } else {
                    (v as u64) << 1
                };
                let byte_len = size_hint(zigzag);
                let mut buf = smallvec![TAG_INTEGER | byte_len];
                let bytes = zigzag.to_be_bytes();
                let slice = &bytes[(8 - byte_len as usize)..];
                buf.extend_from_slice(slice);
                acc.push((path_buf.clone(), buf));
            } else if let Some(v) = v.as_f64() {
                let mut buf = smallvec![TAG_FLOAT];
                buf.extend_from_slice(&v.to_le_bytes());
                acc.push((path_buf.clone(), buf));
            } else {
                panic!("Unsupported number type");
            }
        }
        serde_json::Value::Bool(v) => {
            // For all other values, we push the current path and value
            let value = smallvec![if *v { TAG_BOOL_TRUE } else { TAG_BOOL_FALSE }];
            acc.push((path_buf.clone(), value));
        }
    }
}
