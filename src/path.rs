use crate::encoding::PrefixEncoder;
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::io::Write;
use std::str::Utf8Error;

#[derive(Clone, Debug, Ord, PartialOrd, PartialEq, Eq, Hash)]
pub struct Path<'a> {
    buf: Cow<'a, [u8]>,
}

impl<'a> Path<'a> {
    pub fn from_slice(buf: &'a [u8]) -> Self {
        Self {
            buf: Cow::Borrowed(buf),
        }
    }

    pub fn from_vec(buf: Vec<u8>) -> Self {
        Self {
            buf: Cow::Owned(buf),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.buf.as_ref()
    }

    pub fn to_owned(&self) -> Path<'static> {
        Path {
            buf: Cow::Owned(self.buf.to_vec()),
        }
    }

    pub fn iter(&self) -> PathIter<'_> {
        PathIter::new(self.as_bytes())
    }

    pub fn as_path_buf(&self) -> PathBuf<Vec<u8>> {
        let buf = Vec::from(self.as_bytes());
        PathBuf::new(buf)
    }
}

impl<'a> Display for Path<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let iter = self.iter();
        write!(f, "$")?;
        for segment in iter {
            match segment {
                Ok(segment) => write!(f, "{}", segment)?,
                Err(e) => {
                    eprintln!("{e}");
                    return Err(std::fmt::Error);
                }
            }
        }
        Ok(())
    }
}

pub struct PathIter<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> PathIter<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn consume_key(&mut self) -> Result<&'a str, PathError> {
        let start = self.pos;
        while self.pos < self.buf.len() && self.buf[self.pos] > MAX_INDEX_BYTES {
            self.pos += 1;
        }

        let key_bytes = &self.buf[start..self.pos];
        match std::str::from_utf8(key_bytes) {
            Ok(key) => Ok(key),
            Err(e) => Err(PathError::InvalidKey(e)),
        }
    }

    fn consume_index(&mut self, byte_len: usize) -> Result<u64, PathError> {
        if byte_len == 0 {
            return Ok(0); // 0 is encoded as a single byte
        }
        if self.pos + byte_len >= self.buf.len() {
            return Err(PathError::Eof);
        }

        let mut buf = [0u8; 8];
        buf[(8 - byte_len)..].copy_from_slice(&self.buf[self.pos..self.pos + byte_len]);
        self.pos += byte_len;
        Ok(u64::from_be_bytes(buf))
    }
}

impl<'a> Iterator for PathIter<'a> {
    type Item = Result<PathSegment<'a>, PathError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.buf.len() {
            return None;
        }

        let tag = self.buf[self.pos];
        self.pos += 1;

        match tag {
            TAG_KEY => Some(self.consume_key().map(PathSegment::Key)),
            TAG_CONT => Some(Ok(PathSegment::Cont)),
            byte_len if byte_len <= MAX_INDEX_BYTES => Some(
                self.consume_index((byte_len & 0b0111) as usize)
                    .map(PathSegment::Index),
            ),
            _ => Some(Err(PathError::UnknownTag(tag))),
        }
    }
}

/// Tags for index path segments, i.e. `$.users[42]`
const TAG_INDEX: u8 = 0b0000_1000;

/// Tags for range path segments, i.e. `$.file[100:]`
const TAG_CONT: u8 = 0b0000_1111;

/// Tags for string field path segments, i.e. `$.user.name`
const TAG_KEY: u8 = 0b0000_0000;

/// Mask used to determine if a byte is utf8 key segment, or index/range segment.
const MAX_INDEX_BYTES: u8 = 0b0000_1111;

#[repr(u8)]
#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq, Hash)]
pub enum PathSegment<'a> {
    Key(&'a str) = TAG_KEY,
    Index(u64) = TAG_INDEX,
    Cont = TAG_CONT,
}

impl<'a> From<&'a str> for PathSegment<'a> {
    fn from(value: &'a str) -> Self {
        PathSegment::Key(value)
    }
}

impl<'a> From<u64> for PathSegment<'a> {
    fn from(value: u64) -> Self {
        PathSegment::Index(value)
    }
}

impl<'a> Display for PathSegment<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PathSegment::Key(key) => write!(f, ".{}", key),
            PathSegment::Index(index) => write!(f, "[{}]", index),
            PathSegment::Cont => write!(f, ".."),
        }
    }
}

#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq, Hash)]
pub struct PathBuf<W> {
    writer: W,
}

impl<W> PathBuf<W> {
    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W> AsRef<W> for PathBuf<W> {
    fn as_ref(&self) -> &W {
        &self.writer
    }
}

impl<W: Write> PathBuf<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn push_key(&mut self, key: &str) -> std::io::Result<()> {
        self.writer.write(&[TAG_KEY])?;
        self.writer.write_all(key.as_bytes())?;
        Ok(())
    }

    pub fn push_index(&mut self, index: u64) -> std::io::Result<()> {
        let byte_len = match index {
            0 => 0, // we can encode 0 as a single byte
            1..=255 => 1,
            256..=65535 => 2,
            65536..=4_294_967_295 => 4,
            4_294_967_296.. => 8,
        };
        self.writer.write_all(&[byte_len | TAG_INDEX])?;
        if byte_len == 0 {
            return Ok(()); // 0 is encoded as a single byte
        }
        let bytes = index.to_be_bytes();
        let slice = &bytes[(8 - byte_len as usize)..];
        self.writer.write_all(slice)?;
        Ok(())
    }

    #[inline]
    pub fn push_continued(&mut self) -> std::io::Result<()> {
        self.writer.write_all(&[TAG_CONT])
    }
}

impl PathBuf<Vec<u8>> {
    pub fn from_iter<'a, I>(iter: I) -> Self
    where
        I: IntoIterator<Item = PathSegment<'a>>,
    {
        let writer = Vec::new();
        let mut path_buf = Self { writer };
        for segment in iter {
            match segment {
                PathSegment::Key(key) => path_buf.push_key(key).unwrap(),
                PathSegment::Index(index) => path_buf.push_index(index).unwrap(),
                PathSegment::Cont => path_buf.push_continued().unwrap(),
            }
        }
        path_buf
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("unsupported path segment tag: {0}")]
    UnknownTag(u8),
    #[error("unexpected end of path data")]
    Eof,
    #[error("invalid path key segment: {0}")]
    InvalidKey(Utf8Error),
    #[error("invalid path index segment: {0}")]
    InvalidIndex(#[from] std::num::TryFromIntError),
    #[error("path length exceeds 32KiB limit")]
    PathTooLong,
}

pub trait Encode {
    fn write_to<W: Write>(self, writer: &mut W) -> std::io::Result<()>;
}

impl<I, B1, B2> Encode for I
where
    I: Iterator<Item = (PathBuf<B1>, B2)>,
    B1: AsRef<[u8]>,
    B2: AsRef<[u8]>,
{
    fn write_to<W: Write>(self, writer: &mut W) -> std::io::Result<()> {
        let mut encoder = PrefixEncoder::new(self);
        encoder.write_to(writer)
    }
}

#[cfg(test)]
mod test {
    use crate::path::{Path, PathBuf, PathSegment};
    use std::collections::BTreeSet;

    #[test]
    fn path_build_parse() {
        let mut path_buf = PathBuf::new(Vec::new());
        path_buf.push_key("users").unwrap();
        path_buf.push_index(42).unwrap();
        path_buf.push_key("name").unwrap();
        let path_bytes = path_buf.into_inner();

        let path = Path::from_vec(path_bytes);
        let mut iter = path.iter();
        assert_eq!(iter.next().unwrap().unwrap(), PathSegment::Key("users"));
        assert_eq!(iter.next().unwrap().unwrap(), PathSegment::Index(42));
        assert_eq!(iter.next().unwrap().unwrap(), PathSegment::Key("name"));
        assert!(iter.next().is_none());
    }

    #[test]
    fn path_keeps_lexical_order() {
        let a = PathBuf::from_iter([PathSegment::Key("users"), 1u64.into(), "name".into()]);
        let b = PathBuf::from_iter([PathSegment::Key("users"), 2u64.into(), "name".into()]);
        let c = PathBuf::from_iter([PathSegment::Key("users"), 300u64.into(), "name".into()]);
        let d = PathBuf::from_iter([PathSegment::Key("users"), 300_000u64.into(), "name".into()]);
        let e = PathBuf::from_iter([PathSegment::Key("users"), "abc".into()]);
        let f = PathBuf::from_iter([PathSegment::Key("user"), "name".into()]);
        let g = PathBuf::from_iter([PathSegment::Key("file"), 0u64.into(), PathSegment::Cont]);
        let h = PathBuf::from_iter([
            PathSegment::Key("file"),
            (u16::MAX as u64).into(),
            PathSegment::Cont,
        ]);
        let ordered = BTreeSet::from_iter([
            a.into_inner(),
            b.into_inner(),
            c.into_inner(),
            d.into_inner(),
            e.into_inner(),
            f.into_inner(),
            g.into_inner(),
            h.into_inner(),
        ]);
        let ordered: Vec<_> = ordered
            .into_iter()
            .map(|path| {
                let path = Path::from_vec(path);
                path.to_string()
            })
            .collect();
        assert_eq!(
            ordered,
            vec![
                "$.file[0]..",
                "$.file[65535]..",
                "$.user.name",
                "$.users.abc", // Note: utf-8 strings come before numbers
                "$.users[1].name",
                "$.users[2].name",
                "$.users[300].name",
                "$.users[300000].name",
            ]
        );
    }
}
