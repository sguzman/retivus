//! Integrate a pre-verified Mexilex study JSONL export into a new source catalog.
//! This command never creates or changes learner cards or review histories.
use retivus::source_adapter::MexilexV1;
use retivus::source_store::SourceCatalog;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or("usage: import-source <study.jsonl> <new.sqlite3> <snapshot>")?;
    let target = args.next().ok_or("missing new catalog path")?;
    let snapshot = args.next().ok_or("missing snapshot identifier")?;
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }
    let mut catalog = SourceCatalog::create_new(Path::new(&target))?;
    let mut file = BufReader::new(File::open(input)?);
    let adapter = MexilexV1::default();
    let count = catalog.import_snapshot(&snapshot, &adapter, &mut file)?;
    let (entries, senses) = catalog.counts("mexilex", &snapshot)?;
    assert_eq!(entries, count as i64);
    println!("Imported {entries} Mexilex entries and {senses} senses into source catalog");
    Ok(())
}
