//! Independent indexed source-catalog lookup benchmark; no learner data is opened.
use retivus::source_store::SourceCatalog;
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().ok_or("usage: benchmark-catalog <catalog.sqlite3>")?);
    if args.next().is_some() {
        return Err("usage: benchmark-catalog <catalog.sqlite3>".into());
    }

    let read_only = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let keys: (String, String, String) = read_only.query_row(
        "SELECT provider, snapshot, entry_id FROM entries ORDER BY entry_id LIMIT 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    drop(read_only);

    let catalog = SourceCatalog::open_existing(&path)?;
    let started = Instant::now();
    for _ in 0..1_000 {
        if catalog.lookup_entry(&keys.0, &keys.1, &keys.2)?.is_none() {
            return Err("indexed catalog entry disappeared during lookup benchmark".into());
        }
    }
    let elapsed = started.elapsed();
    if elapsed > Duration::from_secs(10) {
        return Err("1000 indexed catalog lookups exceeded the 10-second benchmark gate".into());
    }
    println!("Performed 1000 indexed source catalog lookups in {} ms", elapsed.as_millis());
    Ok(())
}
