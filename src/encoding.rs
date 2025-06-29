use std::io::Write;
use std::iter;
use crate::Value;

pub struct PrefixEncoder<I> {
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

impl<I, B> PrefixEncoder<I>
where
    I: Iterator<Item = (B, Value)>,
    B: AsRef<[u8]>
{
    pub fn write_to<W: Write>(self, writer: &mut W) -> std::io::Result<()> {
        let mut encoder = self;
        while encoder.write_next(writer)? {
            // Continue writing until there are no more items
        }
        Ok(())
    }

    pub fn write_next<W: Write>(&mut self, buf: &mut W) -> std::io::Result<bool> {
        if let Some((key, value)) = self.iter.next() {
            let key = key.as_ref();
            let prefix_len = mismatch(&self.last_key, &key);
            buf.write_all(&(key.len() as u16).to_be_bytes())?;
            buf.write_all(&(prefix_len as u16).to_be_bytes())?;
            buf.write_all(&key[prefix_len..])?;
            value.write_to(buf)?;
            self.last_key = key.into();
            Ok(true)
        } else {
            // No more items to write
            Ok(false)
        }
    }
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
