//! Import a fully specified independent study TSV into an isolated source catalog.
//! No Anki collection, cards, or learner history is opened.
use retivus::source_adapter::{SourceInfo, StudyTsvV1};
use retivus::source_store::SourceCatalog;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut next = || args.next().ok_or("usage: import-generic <input.tsv> <new-catalog.sqlite3> <snapshot> <provider> <edition> <source-language> <meaning-language> <license-notice>");
    let input = next()?;
    let target = next()?;
    let snapshot = next()?;
    let provider = next()?;
    let edition = next()?;
    let lexical_language = next()?;
    let definition_language = next()?;
    let license_notice = next()?;
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }
    let adapter = StudyTsvV1::new(SourceInfo {
        provider_id: provider.clone(),
        edition,
        lexical_language,
        definition_language,
        license_notice,
    })?;
    let mut catalog = SourceCatalog::create_new(Path::new(&target))?;
    let mut reader = BufReader::new(File::open(input)?);
    let count = catalog.import_snapshot(&snapshot, &adapter, &mut reader)?;
    let (entries, senses) = catalog.counts(&provider, &snapshot)?;
    if entries != count as i64 {
        return Err("stored entry count did not match validated source".into());
    }
    println!("Imported {entries} entries and {senses} senses from independent provider {provider}");
    Ok(())
}
