use crate::Path;
use std::io::{Read, Write};
use std::iter;

pub(crate) struct PrefixEncoder<W> {
    last_key: Vec<u8>,
    writer: W,
}

impl<W> PrefixEncoder<W> {
    pub fn new(writer: W) -> Self {
        Self {
            last_key: Vec::new(),
            writer,
        }
    }
}

/// Maximum allowed length of a path is 32KiB.
const MAX_PATH_LEN: usize = 0x0111_1111_1111_1111;

/// Special bit to indicate that the entry is using extension format.
/// Extension format is reserved to the future use, but current decoder needs to be aware of it
/// in order to correctly decode the entries.
const EXT_ENTRY: u8 = 0b1000_0000;

impl<W: Write> PrefixEncoder<W> {
    pub fn write_next(&mut self, key: &[u8], value: &[u8]) -> std::io::Result<()> {
        debug_assert!(key.len() <= MAX_PATH_LEN);
        debug_assert!(value.len() <= u16::MAX as usize);

        let prefix_len = common_prefix(&self.last_key, &key);

        // write entry header - length of key, of shared prefix between last key and current key
        // and finally length of value
        self.writer.write_all(&(key.len() as u16).to_be_bytes())?;
        self.writer.write_all(&(value.len() as u16).to_be_bytes())?;
        self.writer.write_all(&(prefix_len as u16).to_be_bytes())?;

        // write key part that differs from the last key
        let diff = &key[prefix_len..];
        self.writer.write_all(diff)?;

        // memorize the new last key
        self.last_key.drain(prefix_len..);
        self.last_key.extend_from_slice(diff);

        // write value
        self.writer.write_all(&value)?;

        Ok(())
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
                let skip_len = (key_len & MAX_PATH_LEN) + value_len;
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
    use crate::path::Encode;
    use crate::{PathBuf, PathSegment};
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

        ordered.clone().into_iter().write_to(&mut buf).unwrap();

        let mut decoder = PrefixDecoder::new(Cursor::new(buf));
        let mut decoded = BTreeMap::new();
        while let Some((path, value)) = decoder.read_next().unwrap() {
            decoded.insert(path.as_path_buf(), value.to_vec());
        }

        assert_eq!(decoded, ordered);
    }

    #[test]
    fn test_prefix_decoder_skip_optional() {
        let mut buf = Vec::new();
        let mut encoder = PrefixEncoder::new(&mut buf);

        let a = PathBuf::from_iter([PathSegment::Key("users"), 1u64.into(), "name".into()]);
        let b = PathBuf::from_iter([PathSegment::Key("users"), 2u64.into(), "name".into()]);
        let c = PathBuf::from_iter([PathSegment::Key("users"), 300u64.into(), "name".into()]);

        encoder.write_next(a.as_ref(), b"a").unwrap();
        encoder.write_next(b.as_ref(), b"b").unwrap();

        // write an optional entry that we will skip
        let unknown_entry = [
            EXT_ENTRY, // EXT_ENTRY bit set
            0, 0, 2,         // we'll skip 2 bytes
            EXT_ENTRY, // EXT_ENTRY bit set for optional entry
            0, 123, 100, // last 2 bytes are the entry body to skip
        ];
        encoder.writer.write_all(&unknown_entry).unwrap();
        encoder.write_next(c.as_ref(), b"c").unwrap();

        let mut decoder = PrefixDecoder::new(Cursor::new(buf));

        let expected =
            BTreeMap::from_iter([(a, b"a".to_vec()), (b, b"b".to_vec()), (c, b"c".to_vec())]);

        let mut decoded = BTreeMap::new();
        while let Some((path, value)) = decoder.read_next().unwrap() {
            decoded.insert(path.as_path_buf(), value.to_vec());
        }

        assert_eq!(decoded, expected);
    }

    #[test]
    fn test_prefix_decoder_fail_unskippable() {
        let mut buf = Vec::new();
        let mut encoder = PrefixEncoder::new(&mut buf);

        let a = PathBuf::from_iter([PathSegment::Key("users"), 1u64.into(), "name".into()]);
        let b = PathBuf::from_iter([PathSegment::Key("users"), 2u64.into(), "name".into()]);
        let c = PathBuf::from_iter([PathSegment::Key("users"), 300u64.into(), "name".into()]);

        encoder.write_next(a.as_ref(), b"a").unwrap();
        encoder.write_next(b.as_ref(), b"b").unwrap();

        // write an optional entry that we will skip
        let unknown_entry = [
            EXT_ENTRY, // EXT_ENTRY bit set
            0, 0, 2, // we'll skip 2 bytes
            0, // EXT_ENTRY bit set for non-optional entry
            0, 123, 100, // last 2 bytes are the entry body to skip
        ];
        encoder.writer.write_all(&unknown_entry).unwrap();
        encoder.write_next(c.as_ref(), b"c").unwrap();

        let mut decoder = PrefixDecoder::new(Cursor::new(buf));

        let (path, value) = decoder.read_next().unwrap().unwrap();
        assert_eq!(path.as_path_buf(), a);
        assert_eq!(value, b"a");

        let (path, value) = decoder.read_next().unwrap().unwrap();
        assert_eq!(path.as_path_buf(), b);
        assert_eq!(value, b"b");

        let res = decoder.read_next().unwrap_err();
        assert_eq!(
            res.kind(),
            std::io::ErrorKind::Unsupported,
            "Expected Unsupported error for unskippable entry"
        );
    }
}
