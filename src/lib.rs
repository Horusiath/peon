mod encoding;
pub mod json;
mod json_path;
mod path;

pub use encoding::{PrefixDecoder, PrefixEncoder};
pub use json_path::JsonPath;
pub use path::{Path, PathBuf, PathSegment};

fn size_hint(n: u64) -> u8 {
    match n {
        0 => 0, // we can encode 0 as a single byte
        1..=255 => 1,
        256..=65535 => 2,
        65536..=4_294_967_295 => 4,
        4_294_967_296.. => 8,
    }
}
