use peon::json::Flatten;
use serde_json::Value;
use std::time::Instant;

fn main() {
    let path = "assets/5MB-min.json";
    let content = std::fs::read_to_string(path).expect("Failed to read the file");
    let start = Instant::now();
    let json: Value = serde_json::from_str(&content).expect("Failed to serialize to JSON");
    println!(
        "original content size: {} bytes {:?}",
        content.len(),
        start.elapsed()
    );

    let flattened: Vec<_> = json.flatten(u16::MAX as usize).into_iter().collect();
    let entry_count = flattened.len();
    let mut buf = Vec::new();
    let mut encoder = peon::PrefixEncoder::new(&mut buf);
    let start = Instant::now();

    for (path, value) in flattened {
        encoder
            .write_next(path.as_ref(), &value)
            .expect("Failed to write next");
    }

    let end = Instant::now();
    println!(
        "written {} entries in {:?} (size: {} bytes)",
        entry_count,
        end.duration_since(start),
        buf.len()
    );

    let mut decoder = peon::PrefixDecoder::new(std::io::Cursor::new(buf));
    let start = Instant::now();
    let mut i = 0;
    while let Some((path, value)) = decoder.read_next().expect("Failed to read next") {
        //println!("{} => {}", path, decode_scalar(value).unwrap());
        i += 1;
    }
    let end = Instant::now();
    println!("read {} entries in {:?}", i, end.duration_since(start));
}
