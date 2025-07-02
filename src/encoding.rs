use crate::{Path, PathBuf};
use std::io::{Read, Write};
use std::iter;

pub(crate) struct PrefixEncoder<I> {
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

/// Maximum allowed length of a path is 32KiB.
const MAX_PATH_LEN: usize = 0x0111_1111_1111_1111;

/// Special bit to indicate that the entry is using extension format.
/// Extension format is reserved to the future use, but current decoder needs to be aware of it
/// in order to correctly decode the entries.
const EXT_ENTRY: u8 = 0b1000_0000;

impl<I, B1, B2> PrefixEncoder<I>
where
    I: Iterator<Item = (PathBuf<B1>, B2)>,
    B1: AsRef<[u8]>,
    B2: AsRef<[u8]>,
{
    pub fn write_to<W: Write>(&mut self, writer: &mut W) -> std::io::Result<()> {
        while self.write_next(writer)? {
            // Continue writing until there are no more items
        }
        Ok(())
    }

    pub fn write_next<W: Write>(&mut self, w: &mut W) -> std::io::Result<bool> {
        if let Some((path, value)) = self.iter.next() {
            let key = path.into_inner();
            let key = key.as_ref();
            let value = value.as_ref();

            debug_assert!(key.len() <= MAX_PATH_LEN);
            debug_assert!(value.len() <= u16::MAX as usize);

            let prefix_len = common_prefix(&self.last_key, &key);

            // write entry header - length of key, of shared prefix between last key and current key
            // and finally length of value
            w.write_all(&(key.len() as u16).to_be_bytes())?;
            w.write_all(&(value.len() as u16).to_be_bytes())?;
            w.write_all(&(prefix_len as u16).to_be_bytes())?;

            // write key part that differs from the last key
            let diff = &key[prefix_len..];
            w.write_all(diff)?;

            // memorize the new last key
            self.last_key.drain(prefix_len..);
            self.last_key.extend_from_slice(diff);

            // write value
            w.write_all(&value)?;
            Ok(true)
        } else {
            // No more items to write
            Ok(false)
        }
    }
}

fn common_prefix(xs: &[u8], ys: &[u8]) -> usize {
    common_prefix_chunked::<128>(xs, ys)
}

/// Vectorized version of the longest common prefix algorithm.
/// Source: https://users.rust-lang.org/t/how-to-find-common-prefix-of-two-byte-slices-effectively/25815/6
fn common_prefix_chunked<const N: usize>(xs: &[u8], ys: &[u8]) -> usize {
    let off = iter::zip(xs.chunks_exact(N), ys.chunks_exact(N))
        .take_while(|(x, y)| x == y)
        .count()
        * N;
    off + iter::zip(&xs[off..], &ys[off..])
        .take_while(|(x, y)| x == y)
        .count()
}

pub(crate) struct PrefixDecoder<R> {
    last_key: Vec<u8>,
    last_value: Vec<u8>,
    reader: R,
}

impl<R: Read> PrefixDecoder<R> {
    pub fn new(reader: R) -> Self {
        Self {
            last_key: Vec::new(),
            last_value: Vec::new(),
            reader,
        }
    }

    #[inline(never)]
    fn skip(reader: &mut R, len: usize) -> std::io::Result<()> {
        let mut remaining = len;
        let mut buf = [0u8; 1024]; // buffer to read and discard
        while remaining > 0 {
            let buf_len = remaining.min(buf.len());
            reader.read_exact(&mut buf[..buf_len])?;
            remaining -= buf_len;
        }
        Ok(())
    }

    pub fn read_next(&mut self) -> std::io::Result<Option<(Path, &[u8])>> {
        let mut header_buf = [0u8; 6];
        match self.reader.read_exact(&mut header_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // No more entries to read
                return Ok(None);
            }
            Err(e) => return Err(e),
        }

        let key_len = u16::from_be_bytes([header_buf[0], header_buf[1]]) as usize;
        let value_len = u16::from_be_bytes([header_buf[2], header_buf[3]]) as usize;
        let prefix_len = u16::from_be_bytes([header_buf[4], header_buf[5]]) as usize;

        if header_buf[0] & EXT_ENTRY != 0 {
            // this is an extension entry, which we do not support yet
            if header_buf[4] & EXT_ENTRY != 0 {
                // this entry is optional, we can skip it
                // we reinterpret the length to skip as i32 (with sign bit cleared)
                let skip_len = ((key_len & 0x7fff) << 16) | value_len;
                Self::skip(&mut self.reader, skip_len)?;

                // read next entry
                return self.read_next();
            } else {
                // this entry is mandatory, but we do not support it yet
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "Received non-optional entry of unsupported type",
                ));
            }
        }

        // make sure key buffer is large enough and read it starting from the prefix offset
        self.last_key.resize(key_len, 0);
        self.reader.read_exact(&mut self.last_key[prefix_len..])?;

        // read value
        if self.last_value.len() < value_len {
            self.last_value.reserve(value_len - self.last_value.len());
        }
        unsafe { self.last_value.set_len(value_len) };
        self.reader.read_exact(&mut self.last_value)?;

        let path = Path::from_slice(&self.last_key);
        let value = self.last_value.as_slice();
        Ok(Some((path, value)))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::PathSegment;
    use std::collections::BTreeMap;
    use std::io::Cursor;

    #[test]
    fn test_common_prefix() {
        let xs = b"hello world";
        let ys = b"hello rust";
        assert_eq!(common_prefix(xs, ys), 6);
    }

    #[test]
    fn test_prefix_encoder() {
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
        let ordered: BTreeMap<PathBuf<Vec<u8>>, Vec<u8>> = BTreeMap::from_iter([
            (a, b"a".into()),
            (b, b"b".into()),
            (c, b"c".into()),
            (d, b"d".into()),
            (e, b"e".into()),
            (f, b"f".into()),
            (g, b"g".into()),
            (h, b"h".into()),
        ]);

        let mut buf = Vec::new();

        let mut encoder = PrefixEncoder::new(ordered.clone().into_iter());
        encoder.write_to(&mut buf).unwrap();

        let mut decoder = PrefixDecoder::new(Cursor::new(buf));
        let mut decoded = BTreeMap::new();
        while let Some((path, value)) = decoder.read_next().unwrap() {
            decoded.insert(path.as_path_buf(), value.to_vec());
        }

        assert_eq!(decoded, ordered);
    }
}
