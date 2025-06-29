use crate::path::PathBuf;
use crate::value::Value;

pub trait Flatten {
    fn flatten(&self) -> Vec<(Vec<u8>, Value)>;
}

impl Flatten for serde_json::Value {
    fn flatten(&self) -> Vec<(Vec<u8>, Value)> {
        let mut result = Vec::new();
        let path = PathBuf::new(Vec::new());
        flatten_inner(self, path, &mut result);
        result
    }
}

fn flatten_inner(
    json: &serde_json::Value,
    path_buf: PathBuf<Vec<u8>>,
    acc: &mut Vec<(Vec<u8>, Value)>,
) {
    match json {
        serde_json::Value::Null => { /* ommit the nulls */ }
        serde_json::Value::Object(values) => {
            for (key, value) in values.iter() {
                let mut new_path = path_buf.clone();
                new_path.push_key(key).unwrap();
                flatten_inner(value, new_path, acc);
            }
        }
        serde_json::Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                let mut new_path = path_buf.clone();
                new_path.push_index(index as u64).unwrap();
                flatten_inner(value, new_path, acc);
            }
        }
        serde_json::Value::Number(number) => {
            let value = if let Some(value) = number.as_i128() {
                Value::Int(value)
            } else if let Some(value) = number.as_f64() {
                Value::Float(value)
            } else {
                // If the number is not an integer or float, we skip it
                unreachable!("number should be either integer or float");
            };
            // If it's a scalar value, we add it to the result
            acc.push((path_buf.into_inner(), value));
        }
        serde_json::Value::String(value) => {
            acc.push((path_buf.into_inner(), Value::from(value.clone())));
        }
        serde_json::Value::Bool(boolean) => {
            acc.push((path_buf.into_inner(), Value::from(*boolean)));
        }
    }
}

#[cfg(test)]
mod test {
    use crate::path::Path;

    #[test]
    fn flatten_json() {
        use crate::json::Flatten;
        use serde_json::json;

        let json = json!({
            "users": [
                {"name": "Alice", "age": 30},
                {"name": "Bob", "age": 25}
            ],
            "active": true
        });

        let flattened = json.flatten();
        assert_eq!(flattened.len(), 5);
        let actual = flattened
            .into_iter()
            .map(|(key, value)| {
                (
                    Path::from_vec(key).to_string(),
                    serde_json::Value::from(value),
                )
            })
            .collect::<Vec<_>>();
        let expected = vec![
            ("$.active".into(), json!(1)), // we encode bool as 1 or 0
            ("$.users[0].age".into(), json!(30)),
            ("$.users[0].name".to_string(), json!("Alice")),
            ("$.users[1].age".into(), json!(25)),
            ("$.users[1].name".into(), json!("Bob")),
        ];
        assert_eq!(actual, expected);
    }
}
