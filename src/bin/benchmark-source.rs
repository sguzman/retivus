//! Repeatable full-source preview memory smoke, with no private learner profile access.
use retivus::preview::{validate_preview, MAX_PREVIEW_ROWS};
use retivus::source_adapter::MexilexV1;
use std::fs::File;
use std::io::BufReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: benchmark-source <verified-study.jsonl>")?;
    if args.next().is_some() {
        return Err("usage: benchmark-source <verified-study.jsonl>".into());
    }
    let mut reader = BufReader::new(File::open(path)?);
    let result = validate_preview(&MexilexV1::default(), &mut reader)?;
    if result.rows.len() > MAX_PREVIEW_ROWS {
        return Err("source preview exceeded the fixed UI row bound".into());
    }
    println!(
        "Validated {} records, retained {} preview rows",
        result.validated_count,
        result.rows.len()
    );
    Ok(())
}
