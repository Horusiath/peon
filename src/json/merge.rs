use crate::{Path, PathSegment};

pub trait PathSetter {
    type Value;
    fn set_at(&mut self, path: &Path, value: Self::Value) -> bool;

    fn remove_at(&mut self, path: &Path) -> bool;
}

impl PathSetter for serde_json::Value {
    type Value = Self;

    fn set_at(&mut self, path: &Path, value: Self::Value) -> bool {
        let mut segments = Vec::new();
        for segment in path.iter() {
            match segment {
                Err(_) => return false, //TODO: Handle error appropriately
                Ok(seg) => segments.push(seg),
            }
        }
        let target = touch(self, &segments);
        *target = value;
        true
    }

    fn remove_at(&mut self, path: &Path) -> bool {
        let mut segments = Vec::new();
        for segment in path.iter() {
            match segment {
                Err(_) => return false, //TODO: Handle error appropriately
                Ok(seg) => segments.push(seg),
            }
        }
        if segments.is_empty() {
            *self = serde_json::Value::Null;
        }
        let target = touch(self, &segments[..segments.len() - 1]);
        match segments.last() {
            Some(PathSegment::Key(key)) if target.is_object() => {
                let obj = target.as_object_mut().unwrap();
                obj.remove(*key);
                true
            }
            Some(PathSegment::Index(index)) if target.is_array() => {
                let arr = target.as_array_mut().unwrap();
                let i = *index as usize;
                if i < arr.len() - 1 {
                    arr[*index as usize] = serde_json::Value::Null;
                    true
                } else if i == arr.len() - 1 {
                    arr.pop();
                    true
                } else {
                    false
                }
            }
            _ => false, // Unsupported segment type
        }
    }
}

fn touch<'a>(root: &'a mut serde_json::Value, path: &[PathSegment]) -> &'a mut serde_json::Value {
    let mut current = root;
    for segment in path.iter() {
        match segment {
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
                if !current.is_array() {
                    *current = serde_json::json!([]);
                }
                let arr = current.as_array_mut().unwrap();
                let index = *index as usize;
                if index >= arr.len() {
                    arr.resize(index + 1, serde_json::Value::Null);
                }
                current = arr.get_mut(index).unwrap();
            }
            _ => continue, // Handle other segments if needed
        }
    }
    current
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
    I: Iterator<Item = (Path<'a>, serde_json::Value)>,
{
    type Value = serde_json::Value;

    fn merge_into(self, acc: &mut Self::Value) {
        for (path, value) in self {
            match value {
                serde_json::Value::Null => acc.remove_at(&path),
                _ => acc.set_at(&path, value),
            };
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
            .flatten()
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
            .flatten()
            .into_iter()
            .map(|(path, value)| (path.into_path(), value))
            .filter(|(path, _)| json_path.is_match(path))
            .merge();
        let expected = json!({
           "users": [
                { "name": "Alice" },
                { "name": "Bob" },
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
