mod filter;
mod parse;

use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq)]
pub struct JsonPath<'a> {
    tokens: Vec<JsonPathToken<'a>>,
}

impl<'a> AsRef<[JsonPathToken<'a>]> for JsonPath<'a> {
    fn as_ref(&self) -> &[JsonPathToken<'a>] {
        &self.tokens
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum JsonPathToken<'a> {
    Root,
    Current,
    Member(&'a str),
    Index(i64),
    Wildcard,
    RecursiveDescend,
    Slice(u64, u64, u64),
    MemberUnion(Vec<&'a str>),
    IndexUnion(Vec<i64>),
}

impl<'a> Display for JsonPathToken<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonPathToken::Root => write!(f, r#"$"#),
            JsonPathToken::Current => write!(f, "@"),
            JsonPathToken::Member(key) => {
                if key.chars().any(char::is_whitespace) {
                    write!(f, "['{}']", key)
                } else {
                    write!(f, ".{}", key)
                }
            }
            JsonPathToken::Index(index) => write!(f, "[{}]", index),
            JsonPathToken::Wildcard => write!(f, ".*"),
            JsonPathToken::RecursiveDescend => write!(f, ".."),
            JsonPathToken::Slice(from, to, by) => write!(f, "[{}:{}:{}]", from, to, by),
            JsonPathToken::MemberUnion(members) => {
                let mut i = members.iter();
                write!(f, "[")?;
                if let Some(m) = i.next() {
                    write!(f, "{}", m)?;
                }
                while let Some(m) = i.next() {
                    write!(f, ", {}", m)?;
                }
                write!(f, "]")
            }
            JsonPathToken::IndexUnion(indices) => {
                let mut i = indices.iter();
                write!(f, "[")?;
                if let Some(m) = i.next() {
                    write!(f, "{}", m)?;
                }
                while let Some(m) = i.next() {
                    write!(f, ", {}", m)?;
                }
                write!(f, "]")
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("{0}")]
    InvalidJsonPath(String),
}

#[cfg(test)]
mod test {
    use crate::PathBuf;
    use crate::json::Flatten;
    use crate::json_path::{JsonPath, JsonPathToken};
    use serde_json::json;

    fn mixed_sample() -> impl Iterator<Item = (PathBuf<Vec<u8>>, serde_json::Value)> {
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
        .flatten()
        .into_iter()
    }

    #[test]
    fn eval_member_partial() {
        let any = mixed_sample();
        let path = JsonPath::parse("$.users[*]friends[*]name").unwrap();
        let values: Vec<_> = any
            .filter(|(p, _)| path.is_match(&p.as_path()))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(values, vec![json!("Bob")]);
    }

    #[test]
    fn eval_member_full() {
        let any = mixed_sample();
        let path = JsonPath::parse("$.users[0].name").unwrap();
        let values: Vec<_> = any
            .filter(|(p, _)| path.is_match(&p.as_path()))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(values, vec![json!("Alice")]);
    }

    #[test]
    fn eval_member_wildcard_array() {
        let any = mixed_sample();
        let path = JsonPath::parse("$.users[*].name").unwrap();
        let values: Vec<_> = any
            .filter(|(p, _)| path.is_match(&p.as_path()))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(
            values,
            vec![
                json!("Alice"),
                json!("Bob"),
                json!("Damian"),
                json!("Elise")
            ]
        );
    }

    #[test]
    fn eval_member_slice() {
        let any = mixed_sample();
        let path = JsonPath::parse("$.users[1:3].nick").unwrap();
        let values: Vec<_> = any
            .filter(|(p, _)| path.is_match(&p.as_path()))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(values, vec![json!("boreas"), json!("crocodile91")]);
    }

    #[test]
    fn eval_index_union() {
        let any = mixed_sample();
        let path = JsonPath::parse("$.users[1,3].name").unwrap();
        let values: Vec<_> = any
            .filter(|(p, _)| path.is_match(&p.as_path()))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(values, vec![json!("Bob"), json!("Damian")]);
    }

    #[test]
    fn eval_member_union() {
        let any = mixed_sample();
        let path = JsonPath::parse("$.users[0]['name','surname']").unwrap();
        let values: Vec<_> = any
            .filter(|(p, _)| path.is_match(&p.as_path()))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(values, vec![json!("Alice"), json!("Smith")]);
    }

    #[test]
    fn eval_descent_flat() {
        let any = mixed_sample();
        let path = JsonPath::parse("$.users..name").unwrap();
        let values: Vec<_> = any
            .filter(|(p, _)| path.is_match(&p.as_path()))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(
            values,
            vec![
                // flattened JSON fields are in alphabetical order
                json!("Bob"),    // $.users[0].friends[0].name
                json!("Alice"),  // $.users[0].name
                json!("Bob"),    // $.users[1].name
                json!("Damian"), // $.users[2].name
                json!("Elise")   // $.users[3].name
            ]
        );
    }

    #[test]
    fn eval_descent_multi_level() {
        let any = json!({
            "a": {
                "b1": {
                    "c": {
                        "f": {
                            "name": "Alice"
                        }
                    }
                },
                "b2": {
                    "d": {
                        "c": {
                            "g": {
                                "h": {
                                    "name": "Bob"
                                }
                            }
                        }
                    }
                }
            }
        })
        .flatten()
        .into_iter();
        let path = JsonPath::parse("$..c..name").unwrap();
        let values: Vec<_> = any
            .filter(|(p, _)| path.is_match(&p.as_path()))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(values, vec![json!("Alice"), json!("Bob")]);
    }
}
