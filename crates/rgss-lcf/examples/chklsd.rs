use rgss_lcf::{parse, LcfPayload};
use std::path::Path;

fn main() {
    let p = Path::new("RM2000_test/game/Save01.lsd");
    let bytes = std::fs::read(p).expect("read");
    println!("len {}", bytes.len());
    match parse(&bytes) {
        Ok(doc) => {
            println!("OK header={:?} chunks={}", String::from_utf8_lossy(&doc.header), doc.chunks.len());
            for c in &doc.chunks {
                let kind = match &c.payload {
                    LcfPayload::Raw(b) => format!("raw({})", b.len()),
                    LcfPayload::Fields(f) => format!("fields({})", f.len()),
                    LcfPayload::StructArray { count, elements } => {
                        format!("array({count}/{})", elements.len())
                    }
                };
                println!("  chunk 0x{:02x}: {}", c.id, kind);
            }
        }
        Err(e) => println!("ERR {e}"),
    }
}
