use crate::ValueRef;
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::io::{Read, Write};
use std::iter;
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
}

impl<'a> Display for Path<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let iter = self.iter();
        write!(f, "$")?;
        for segment in iter {
            match segment {
                Ok(segment) => write!(f, "{}", segment)?,
                Err(e) => return Err(std::fmt::Error),
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
            TAG_CHUNK => {}
            byte_len if byte_len <= MAX_INDEX_BYTES => Some(
                self.consume_index(byte_len as usize)
                    .map(PathSegment::Index),
            ),
            _ => Some(Err(PathError::UnknownTag(tag))),
        }
    }
}

const TAG_KEY: u8 = 0;
const TAG_INDEX: u8 = 1;
const TAG_CHUNK: u8 = 2;
const MAX_INDEX_BYTES: u8 = 8;

#[repr(u8)]
#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq, Hash)]
pub enum PathSegment<'a> {
    Key(&'a str) = TAG_KEY,
    Index(u64) = TAG_INDEX,
    ByteChunk(u64, u16) = TAG_CHUNK,
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
            PathSegment::ByteChunk(start, len) => write!(f, "[{}:{}]", start, *start + *len as u64),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PathBuf<W> {
    writer: W,
}

impl<W> PathBuf<W> {
    pub fn into_inner(self) -> W {
        self.writer
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
        write_len_prefixed_varint(&mut self.writer, index)
    }

    pub fn push_chunk(&mut self, start: u64, len: u16) -> std::io::Result<()> {
        self.writer.write(&[TAG_CHUNK])?;
        write_len_prefixed_varint(&mut self.writer, start)?;
        self.writer.write_all(&len.to_be_bytes())?;
        Ok(())
    }
}

fn write_len_prefixed_varint<W: Write>(w: &mut W, value: u64) -> std::io::Result<()> {
    let byte_len = match value {
        0 => return w.write_all(&[0]), // we can encode 0 as a single byte
        1..=255 => 1,
        256..=65535 => 2,
        65536..=4_294_967_295 => 4,
        4_294_967_296.. => 8,
    };
    w.write_all(&[byte_len])?;
    let bytes = value.to_be_bytes();
    let slice = &bytes[(8 - byte_len as usize)..];
    w.write_all(slice)?;
    Ok(())
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
                PathSegment::ByteChunk(start, len) => path_buf.push_chunk(start, len).unwrap(),
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
}

pub trait Encode {
    fn write_to<W: Write>(self, writer: &mut W) -> std::io::Result<()>;
}

impl<'a, I, B> Encode for I
where
    I: Iterator<Item = (PathBuf<B>, ValueRef<'a>)>,
    B: AsRef<[u8]>,
{
    fn write_to<W: Write>(self, writer: &mut W) -> std::io::Result<()> {
        let mut encoder = PrefixEncoder::new(self);
        encoder.write_to(writer)
    }
}

struct PrefixEncoder<I> {
    last_key: Vec<u8>,
    iter: I,
}

impl<I> PrefixEncoder<I> {
    pub fn new(iter: I) -> Self {
        Self {
            last_key: Vec::new(),
            iter,
        }
    }
}

impl<'a, I, B> PrefixEncoder<I>
where
    I: Iterator<Item = (PathBuf<B>, ValueRef<'a>)>,
    B: AsRef<[u8]>,
{
    pub fn write_to<W: Write>(self, writer: &mut W) -> std::io::Result<()> {
        let mut encoder = self;
        while encoder.write_next(writer)? {
            // Continue writing until there are no more items
        }
        Ok(())
    }

    pub fn write_next<W: Write>(&mut self, w: &mut W) -> std::io::Result<bool> {
        if let Some((path, value)) = self.iter.next() {
            let key = path.into_inner();
            let key = key.as_ref();
            let prefix_len = mismatch(&self.last_key, &key);
            w.write_all(&(key.len() as u16).to_be_bytes())?;
            w.write_all(&(prefix_len as u16).to_be_bytes())?;
            w.write_all(&key[prefix_len..])?;
            self.last_key = key.into();
            self.write_value(w, value)?;
            Ok(true)
        } else {
            // No more items to write
            Ok(false)
        }
    }

    fn write_value<W: Write>(&self, w: &mut W, value: ValueRef) -> std::io::Result<()> {
        match value {
            ValueRef::ByteChunk(chunk) => {
                debug_assert!(chunk.len() <= u16::MAX as usize);
                w.write_all(chunk)
            }
            ValueRef::VarInt(i) => encode_varint(w, i),
        }
    }
}

fn encode_varint<W: Write>(w: &mut W, n: i128) -> std::io::Result<()> {
    let mut n = zigzag_encode(n);
    while n >= 0x80 {
        w.write_all(&[MSB | (n as u8)])?;
        n >>= 7;
    }
    w.write_all(&[(n as u8) & DROP_MSB])
}

fn decode_varint<R: Read>(r: &mut R) -> std::io::Result<i128> {
    let mut n: u128 = 0;
    let mut shift = 0;
    loop {
        let mut byte = [0u8; 1];
        r.read_exact(&mut byte)?;
        let byte = byte[0];
        n |= ((byte & DROP_MSB) as u128) << shift;
        if byte & MSB == 0 {
            break;
        }
        shift += 7;
    }
    Ok(zigzag_decode(n))
}

/// Most-significant byte, == 0x80
pub const MSB: u8 = 0b1000_0000;
/// All bits except for the most significant. Can be used as bitmask to drop the most-signficant
/// bit using `&` (binary-and).
const DROP_MSB: u8 = 0b0111_1111;

#[inline]
fn zigzag_encode(n: i128) -> u128 {
    ((n << 1) ^ (n >> 127)) as u128
}

#[inline]
fn zigzag_decode(from: u128) -> i128 {
    ((from >> 1) ^ (-((from & 1) as i128)) as u128) as i128
}

fn mismatch(xs: &[u8], ys: &[u8]) -> usize {
    mismatch_chunks::<128>(xs, ys)
}

/// Vectorized version of the longest common prefix algorithm.
/// Source: https://users.rust-lang.org/t/how-to-find-common-prefix-of-two-byte-slices-effectively/25815/6
fn mismatch_chunks<const N: usize>(xs: &[u8], ys: &[u8]) -> usize {
    let off = iter::zip(xs.chunks_exact(N), ys.chunks_exact(N))
        .take_while(|(x, y)| x == y)
        .count()
        * N;
    off + iter::zip(&xs[off..], &ys[off..])
        .take_while(|(x, y)| x == y)
        .count()
}

struct PrefixDecoder<R> {
    last_key: Vec<u8>,
    reader: R,
}

impl<R: Read> PrefixDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            last_key: Vec::new(),
            reader,
        }
    }

    pub fn read_next(&mut self) -> std::io::Result<Option<(PathBuf<Vec<u8>>, ValueRef)>> {
        let mut key_len_bytes = [0u8; 2];
        if self.reader.read_exact(&mut key_len_bytes).is_err() {
            return Ok(None); // EOF
        }
        let key_len = u16::from_be_bytes(key_len_bytes) as usize;

        let mut prefix_len_bytes = [0u8; 2];
        self.reader.read_exact(&mut prefix_len_bytes)?;
        let prefix_len = u16::from_be_bytes(prefix_len_bytes) as usize;

        let mut key = vec![0u8; key_len];
        key.copy_from_slice(&self.last_key[..prefix_len]);
        self.reader.read_exact(&mut key[prefix_len..])?;
        Path::from_vec(key);

        let value = decode_varint(&mut self.reader)?;

        self.last_key = key.clone();
        Ok(Some((
            PathBuf::from_iter(
                [PathSegment::Key(Cow::Owned(String::from_utf8_lossy(&key)))].into_iter(),
            ),
            ValueRef::VarInt(value),
        )))
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
        let ordered = BTreeSet::from_iter([
            a.into_inner(),
            b.into_inner(),
            c.into_inner(),
            d.into_inner(),
            e.into_inner(),
            f.into_inner(),
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
