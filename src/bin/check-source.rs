//! Read-only provider smoke CLI, used to validate a published source snapshot.
use retivus::provider::visit_mexilex;
use std::fs::File;
use std::io::BufReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: check-source <mexilex-study.jsonl>")?;
    if args.next().is_some() {
        return Err("usage: check-source <mexilex-study.jsonl>".into());
    }
    let count = visit_mexilex(BufReader::new(File::open(path)?), |_| {})?;
    println!("Validated {count} Mexilex entries (no cards or review data created)");
    Ok(())
}
