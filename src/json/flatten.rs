use crate::PathBuf;
use serde_json::Value;

pub trait Flatten {
    type Value;
    type Iter: IntoIterator<Item = (PathBuf<Vec<u8>>, Self::Value)>;
    /// Flattens the JSON structure into a single-level map.
    ///
    /// The keys in the map are JSON paths, and the values are the corresponding JSON values.
    fn flatten(self) -> Self::Iter;
}

impl Flatten for serde_json::Value {
    type Value = serde_json::Value;
    type Iter = Vec<(PathBuf<Vec<u8>>, Self::Value)>;

    fn flatten(self) -> Self::Iter {
        let mut acc = Vec::new();
        let mut path = PathBuf::new(Vec::new());

        flatten_inner(&self, &mut path, &mut acc);

        acc
    }
}

fn flatten_inner(
    value: &Value,
    path_buf: &mut PathBuf<Vec<u8>>,
    acc: &mut Vec<(PathBuf<Vec<u8>>, Value)>,
) {
    match value {
        Value::Null => {} // Null values are ignored
        Value::Array(array) => {
            for (index, item) in array.iter().enumerate() {
                let mut path_buf = path_buf.clone();
                // Push the current index to the path
                path_buf.push_index(index as u64).unwrap();
                // Recursively flatten the item
                flatten_inner(item, &mut path_buf, acc);
            }
        }
        Value::Object(array) => {
            for (key, item) in array.iter() {
                let mut path_buf = path_buf.clone();
                // Push the current key to the path
                path_buf.push_key(key).unwrap();
                // Recursively flatten the item
                flatten_inner(item, &mut path_buf, acc);
            }
        }
        value => {
            // For all other values, we push the current path and value
            acc.push((path_buf.clone(), value.clone()));
        }
    }
}
