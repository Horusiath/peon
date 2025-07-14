use crate::json_path::JsonPathToken;
use crate::{JsonPath, Path, PathSegment};

impl<'a> JsonPath<'a> {
    pub fn is_match(&self, path: &Path) -> bool {
        let mut iter = Vec::new();
        for segment in path.iter() {
            match segment {
                Ok(seg) => iter.push(seg),
                Err(_) => return false, // If there's an error, we can't match
            }
        }

        match_path_inner(self.as_ref(), 0, &iter, 0)
    }
}

fn match_path_inner<'a>(
    tokens: &[JsonPathToken<'a>],
    mut token_index: usize,
    path: &[PathSegment<'a>],
    mut path_index: usize,
) -> bool {
    while token_index < tokens.len() {
        match tokens[token_index] {
            JsonPathToken::Root => {
                path_index = 0; // Reset path iterator to the start
            }
            JsonPathToken::Current => {
                // Current token matches any current position in the path
            }
            JsonPathToken::Member(key1) => {
                if let Some(PathSegment::Key(key2)) = path.get(path_index) {
                    if key1 != *key2 {
                        return false; // Member key does not match
                    }
                } else {
                    return false; // No more segments to match or segment is not a key
                }
                path_index += 1;
            }
            JsonPathToken::Index(index1) => {
                if let Some(PathSegment::Index(index2)) = path.get(path_index) {
                    if index1 != *index2 as i64 {
                        return false; // Index does not match
                    }
                } else {
                    return false; // No more segments to match
                }
                path_index += 1;
            }
            JsonPathToken::Wildcard => {
                path_index += 1; // Wildcard matches any segment, so we just continue
            }
            JsonPathToken::RecursiveDescend => {
                // Recursive descend logic can be complex, simplified here
                for i in path_index..path.len() {
                    if match_path_inner(tokens, token_index + 1, path, i) {
                        return true; // Found a match deeper in the path
                    }
                }
            }
            JsonPathToken::Slice(from, to, _) => {
                // Slice logic would require more context about the path structure
                // Simplified for now, assuming it matches any segment in range
                let range = from..to;
                match path.get(path_index) {
                    Some(PathSegment::Index(i)) if range.contains(&i) => { /* continue */ }
                    _ => return false,
                }
                path_index += 1; // Move to the next segment
            }
            JsonPathToken::MemberUnion(ref keys) => {
                if let Some(PathSegment::Key(key)) = path.get(path_index) {
                    if !keys.contains(&key) {
                        return false; // Segment not in member union
                    }
                } else {
                    return false; // No more segments to match
                }
                path_index += 1; // Move to the next segment
            }
            JsonPathToken::IndexUnion(ref indices) => {
                if let Some(PathSegment::Index(index)) = path.get(path_index) {
                    if !indices.contains(&(*index as i64)) {
                        return false; // Segment index not in index union
                    }
                } else {
                    return false; // No more segments to match
                }
                path_index += 1; // Move to the next segment
            }
        }
        token_index += 1;
    }

    true
}
