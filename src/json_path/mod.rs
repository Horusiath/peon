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
    use crate::json::{Flatten, TAG_STRING};
    use crate::json_path::JsonPath;
    use serde_json::json;
    use smallvec::SmallVec;

    fn mixed_sample() -> impl Iterator<Item = (PathBuf<Vec<u8>>, SmallVec<u8, 10>)> {
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
        .flatten(100)
        .into_iter()
    }

    const BYTESTRING_BOB: &'static [u8] = &[TAG_STRING, b'B', b'o', b'b'];
    const BYTESTRING_ALICE: &'static [u8] = &[TAG_STRING, b'A', b'l', b'i', b'c', b'e'];
    const BYTESTRING_DAMIAN: &'static [u8] = &[TAG_STRING, b'D', b'a', b'm', b'i', b'a', b'n'];
    const BYTESTRING_ELISE: &'static [u8] = &[TAG_STRING, b'E', b'l', b'i', b's', b'e'];
    const BYTESTRING_BOREAS: &'static [u8] = &[TAG_STRING, b'b', b'o', b'r', b'e', b'a', b's'];
    const BYTESTRING_CROCODILE91: &'static [u8] = &[
        TAG_STRING, b'c', b'r', b'o', b'c', b'o', b'd', b'i', b'l', b'e', b'9', b'1',
    ];
    const BYTESTRING_SMITH: &'static [u8] = &[TAG_STRING, b'S', b'm', b'i', b't', b'h'];

    #[test]
    fn eval_member_partial() {
        let any = mixed_sample();
        let path = JsonPath::parse("$.users[*]friends[*]name").unwrap();
        let values: Vec<_> = any
            .filter(|(p, _)| path.is_match(&p.as_path()))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(values, vec![BYTESTRING_BOB]);
    }

    #[test]
    fn eval_member_full() {
        let any = mixed_sample();
        let path = JsonPath::parse("$.users[0].name").unwrap();
        let values: Vec<_> = any
            .filter(|(p, _)| path.is_match(&p.as_path()))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(values, vec![BYTESTRING_ALICE]);
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
                SmallVec::<u8, 10>::from_slice(BYTESTRING_ALICE),
                SmallVec::from_slice(BYTESTRING_BOB),
                SmallVec::from_slice(BYTESTRING_DAMIAN),
                SmallVec::from_slice(BYTESTRING_ELISE),
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
        assert_eq!(
            values,
            vec![
                SmallVec::<u8, 10>::from_slice(BYTESTRING_BOREAS),
                SmallVec::from_slice(BYTESTRING_CROCODILE91)
            ]
        );
    }

    #[test]
    fn eval_index_union() {
        let any = mixed_sample();
        let path = JsonPath::parse("$.users[1,3].name").unwrap();
        let values: Vec<_> = any
            .filter(|(p, _)| path.is_match(&p.as_path()))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(
            values,
            vec![
                SmallVec::<u8, 10>::from_slice(BYTESTRING_BOB),
                SmallVec::from_slice(BYTESTRING_DAMIAN)
            ]
        );
    }

    #[test]
    fn eval_member_union() {
        let any = mixed_sample();
        let path = JsonPath::parse("$.users[0]['name','surname']").unwrap();
        let values: Vec<_> = any
            .filter(|(p, _)| path.is_match(&p.as_path()))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(
            values,
            vec![
                SmallVec::<u8, 10>::from_slice(BYTESTRING_ALICE),
                SmallVec::from_slice(BYTESTRING_SMITH)
            ]
        );
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
                SmallVec::<u8, 10>::from_slice(BYTESTRING_BOB), // $.users[0].friends[0].name
                SmallVec::from_slice(BYTESTRING_ALICE),         // $.users[0].name
                SmallVec::from_slice(BYTESTRING_BOB),           // $.users[1].name
                SmallVec::from_slice(BYTESTRING_DAMIAN),        // $.users[2].name
                SmallVec::from_slice(BYTESTRING_ELISE)          // $.users[3].name
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
        .flatten(100)
        .into_iter();
        let path = JsonPath::parse("$..c..name").unwrap();
        let values: Vec<_> = any
            .filter(|(p, _)| path.is_match(&p.as_path()))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(
            values,
            vec![
                SmallVec::<u8, 10>::from_slice(BYTESTRING_ALICE),
                SmallVec::from_slice(BYTESTRING_BOB)
            ]
        );
    }
}
