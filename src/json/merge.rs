use crate::json::TAG_INTEGER;
use crate::{Path, PathSegment};
use std::cmp::Ordering;

fn touch<'a>(root: &'a mut serde_json::Value, path: &Path) -> (&'a mut serde_json::Value, usize) {
    let mut current = root;
    let mut offset = 0;
    let mut cont = false;
    for segment in path.iter() {
        match segment.unwrap() {
            PathSegment::Key(key) => {
                if !current.is_object() {
                    *current = serde_json::json!({});
                }
                let obj = current.as_object_mut().unwrap();
                current = obj
                    .entry(key.to_string())
                    .or_insert(serde_json::Value::Null);
            }
            PathSegment::Index(index) => {
                if !current.is_array() && index == 0 {
                    *current = serde_json::json!([]);
                }
                let arr = current.as_array_mut().unwrap();
                offset = index as usize;
                if offset >= arr.len() {
                    arr.resize(offset + 1, serde_json::Value::Null);
                }
                current = arr.get_mut(offset).unwrap();
            }
            PathSegment::Cont => {
                cont = true;
                if !current.is_string() && offset == 0 {
                    *current == serde_json::Value::String("".into());
                }
            }
        }
    }
    if !cont {
        offset = 0;
    }
    (current, offset)
}

pub trait Merge: Sized {
    type Value: Default;

    fn merge_into(self, acc: &mut Self::Value);

    fn merge(self) -> Self::Value {
        let mut acc = Self::Value::default();
        self.merge_into(&mut acc);
        acc
    }
}

impl<'a, I> Merge for I
where
    I: Iterator<Item = (Path<'a>, super::Value)>,
{
    type Value = serde_json::Value;

    fn merge_into(self, acc: &mut Self::Value) {
        for (path, value) in self {
            let (target, offset) = touch(acc, &path);
            if offset > 0 {
                if let serde_json::Value::String(str) = target {
                    let string_value = str::from_utf8(&value).unwrap();
                    match offset.cmp(&string_value.len()) {
                        Ordering::Less => str.replace_range(offset.., string_value),
                        Ordering::Equal => str.push_str(string_value),
                        Ordering::Greater => panic!(
                            "cannot merge string at offset {} of string which len is {}",
                            offset,
                            str.len()
                        ),
                    }
                }
                continue;
            }
            let tag = value[0];
            match tag {
                super::TAG_NULL => {
                    *target = serde_json::Value::Null;
                }
                super::TAG_STRING => {
                    let bytes = &value[1..];
                    let string_value = String::from_utf8_lossy(bytes).to_string();
                    *target = serde_json::Value::String(string_value);
                    continue;
                }
                super::TAG_FLOAT => {
                    let number: f64 = f64::from_be_bytes(value[1..].try_into().unwrap());
                    *target = number.into();
                }
                super::TAG_BOOL_TRUE => {
                    *target = serde_json::Value::Bool(true);
                }
                super::TAG_BOOL_FALSE => {
                    *target = serde_json::Value::Bool(false);
                }
                tag => {
                    let len = (tag & 0b0000_1111) as usize;
                    let bytes = &value[1..1 + len];
                    let mut zigzag: u64 = 0;
                    for byte in bytes.iter().rev() {
                        zigzag = (zigzag << 8) | *byte as u64;
                    }
                    let number = if zigzag & 1 == 0 {
                        (zigzag >> 1) as i64
                    } else {
                        !((zigzag >> 1) as i64)
                    };
                    *target = number.into();
                }
                _ => {
                    // Handle unknown tags or unsupported types
                    continue;
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::JsonPath;
    use crate::json::{Flatten, Merge};
    use serde_json::json;

    #[test]
    fn flatten_merge() {
        let expected = mixed_sample();
        let actual = expected
            .clone()
            .flatten(100)
            .into_iter()
            .map(|(path, value)| (path.into_path(), value))
            .merge();
        assert_eq!(actual, expected);
    }

    #[test]
    fn flatten_filter_merge() {
        let json_path = JsonPath::parse("users[*].name").unwrap();
        let source = mixed_sample();
        let actual = source
            .clone()
            .flatten(100)
            .into_iter()
            .map(|(path, value)| (path.into_path(), value))
            .filter(|(path, _)| json_path.is_match(path))
            .merge();
        let expected = json!({
           "users": [
                { "name": "Alice" },
                { "name": "Bob" },
                null, // $.users[2] is missing in filter results, but we need it in order to merge remaining parts
                { "name": "Damian" },
                { "name": "Elise" }
            ]
        });
        assert_eq!(actual, expected);
    }

    fn mixed_sample() -> serde_json::Value {
        json!({
            "users": [
                {
                    "name": "Alice",
                    "surname": "Smith",
                    "age": 25,
                    "friends": [
                        { "name": "Bob", "nick": "boreas" },
                        { "nick": "crocodile91" }
                    ]
                },
                {
                    "name": "Bob",
                    "nick": "boreas",
                    "age": 30
                },
                {
                    "nick": "crocodile91",
                    "age": 35
                },
                {
                    "name": "Damian",
                    "surname": "Smith",
                    "age": 30
                },
                {
                    "name": "Elise",
                    "age": 35
                }
            ]
        })
    }
}
