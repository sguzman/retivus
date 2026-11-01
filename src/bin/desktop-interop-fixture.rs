//! Reproducible offline package for independent Anki Desktop GUI verification.
//! Does not use, discover or modify normal Anki profiles.
use retivus::anki_backend::AnkiBackend;
use std::error::Error;
use std::path::{Path, PathBuf};

const PACKAGE: &str = "retivus-desktop-interop.apkg";
const DECK: &str = "Retivus Desktop Interoperability";
const BIDIRECTIONAL: &str = "Retivus QA Two-Way";

fn expected_fields() -> Vec<Vec<String>> {
    let mut fields = vec![
        vec!["mañana".into(), "tomorrow".into()],
        vec!["橋".into(), "bridge".into()],
        vec!["<b>solar</b>".into(), "sun".into()],
        vec!["uno".into(), "one".into()],
    ];
    fields.sort();
    fields
}

fn generate(folder: &Path) -> Result<PathBuf, Box<dyn Error>> {
    if !folder.is_absolute() || !folder.is_dir() {
        return Err("generate requires an existing absolute output directory".into());
    }
    let folder = folder.canonicalize()?;
    let staging = tempfile::tempdir_in(&folder)?;
    let mut backend = AnkiBackend::create_managed_profile(staging.path(), "fixture")?;
    let deck = backend.create_deck(DECK)?;
    backend.add_basic_note(deck, "mañana", "tomorrow")?;
    backend.add_basic_note(deck, "橋", "bridge")?;
    backend.add_basic_note(deck, "<b>solar</b>", "sun")?;
    backend.create_bidirectional_notetype(BIDIRECTIONAL)?;
    backend.add_note_with_notetype(deck, BIDIRECTIONAL, &["uno", "one"])?;
    let stats = backend.collection_stats()?;
    if (stats.notes, stats.cards, stats.review_events) != (4, 5, 0) {
        return Err(format!("unexpected fixture stats: {stats:?}").into());
    }
    let path = folder.join(PACKAGE);
    if backend.export_apkg_to(&path)? != 4 {
        return Err("Anki exporter failed to include four fixture notes".into());
    }
    backend.close()?;
    Ok(path)
}

fn verify(package: &Path) -> Result<(), Box<dyn Error>> {
    if !package.is_absolute() || !package.is_file() {
        return Err("verify requires an existing absolute Anki Desktop export .apkg".into());
    }
    let staging = tempfile::tempdir()?;
    let mut backend = AnkiBackend::create_managed_profile(staging.path(), "reimport")?;
    backend.import_apkg(package)?;
    let stats = backend.collection_stats()?;
    if (stats.notes, stats.cards, stats.review_events) != (4, 5, 0) {
        return Err(format!("Anki Desktop export changed the fixture totals: {stats:?}").into());
    }
    let mut fields = backend
        .browse_notes("", 100)?
        .into_iter()
        .map(|note| note.fields)
        .collect::<Vec<_>>();
    fields.sort();
    if fields != expected_fields() {
        return Err("Anki Desktop export changed one or more exact note fields".into());
    }
    if !backend.list_decks()?.iter().any(|(_, name)| name == DECK) {
        return Err("Anki Desktop export omitted the dedicated fixture deck".into());
    }
    if !backend
        .list_notetypes()?
        .iter()
        .any(|(name, templates)| name == BIDIRECTIONAL && *templates == 2)
    {
        return Err("Anki Desktop export omitted the two-card note type".into());
    }
    backend.close()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let mode = args
        .next()
        .ok_or("usage: desktop-interop-fixture <generate|verify> <absolute path>")?;
    let path = PathBuf::from(args.next().ok_or("missing absolute path")?);
    if args.next().is_some() {
        return Err("expected exactly one subcommand and one absolute path".into());
    }
    match mode.as_str() {
        "generate" => println!("Generated: {}", generate(&path)?.display()),
        "verify" => {
            verify(&path)?;
            println!(
                "Verified Desktop-exported fixture: 4 notes, 5 cards, preserved fields/deck/template"
            );
        }
        _ => return Err("expected generate or verify".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposable_fixture_round_trips_without_touching_existing_files() {
        let root = tempfile::tempdir().unwrap();
        let output = generate(root.path()).unwrap();
        assert!(output.is_file());
        verify(&output).unwrap();
        assert!(generate(root.path()).is_err(), "existing fixture cannot be overwritten");
        verify(&output).unwrap();
        assert!(verify(&root.path().join("missing.apkg")).is_err());
    }
}
