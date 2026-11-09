//! Repeatable full-source preview memory smoke, with no private learner profile access.
use retivus::preview::{search_preview, validate_preview, MAX_PREVIEW_ROWS};
use retivus::source_adapter::MexilexV1;
use std::fs::File;
use std::io::BufReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or("usage: benchmark-source <verified-study.jsonl>")?;
    if args.next().is_some() {
        return Err("usage: benchmark-source <verified-study.jsonl>".into());
    }
    let mut reader = BufReader::new(File::open(&path)?);
    let result = validate_preview(&MexilexV1::default(), &mut reader)?;
    if result.rows.len() > MAX_PREVIEW_ROWS {
        return Err("source preview exceeded the fixed UI row bound".into());
    }
    println!(
        "Validated {} records, retained {} preview rows",
        result.validated_count,
        result.rows.len()
    );
    // Also exercise a real bounded search on the full independently verified
    // Mexilex release, not just unit-test fixtures. No cards are created.
    let mut reader = BufReader::new(File::open(path)?);
    let search = search_preview(&MexilexV1::default(), &mut reader, "vida")?;
    if search.validated_count != result.validated_count
        || search.matching_count == 0
        || search.rows.len() > MAX_PREVIEW_ROWS
    {
        return Err("full-release vocabulary search failed its integrity/bound checks".into());
    }
    println!(
        "Matched {} of {} source records for vida, retained {} search rows",
        search.matching_count,
        search.validated_count,
        search.rows.len()
    );
    Ok(())
}
