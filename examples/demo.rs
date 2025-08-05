use peon::json::Flatten;
use serde_json::Value;
use std::time::Instant;

fn main() {
    let path = "assets/5MB-min.json";
    let content = std::fs::read_to_string(path).expect("Failed to read the file");
    println!("original content size: {} bytes", content.len());

    let json: Value = serde_json::from_str(&content).expect("Failed to serialize to JSON");

    let flattened: Vec<_> = json.flatten().into_iter().collect();
    let entry_count = flattened.len();
    let mut buf = Vec::new();
    let mut value_buf = Vec::new();
    let mut encoder = peon::PrefixEncoder::new(&mut buf);
    let start = Instant::now();

    for (path, value) in flattened {
        serde_json::to_writer(&mut value_buf, &value).expect("Failed to serialize to JSON");
        encoder
            .write_next(path.as_ref(), &value_buf)
            .expect("Failed to write next");
        value_buf.clear();
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
        i += 1;
    }
    let end = Instant::now();
    println!("read {} entries in {:?}", i, end.duration_since(start));
}
