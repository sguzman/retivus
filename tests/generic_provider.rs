//! Cross-format acceptance: a non-Mexilex source must be importable and reviewable.
use retivus::preview::validate_preview;
use retivus::source_adapter::{SourceAdapter, SourceInfo, StudyTsvV1};
use retivus::source_store::SourceCatalog;
use std::io::Cursor;
use tempfile::tempdir;

const TSV: &str = include_str!("fixtures/independent-ja-es.tsv");

fn adapter() -> StudyTsvV1 {
    StudyTsvV1::new(SourceInfo {
        provider_id: "independent-ja-es".into(),
        edition: "2026-11-01".into(),
        lexical_language: "ja".into(),
        definition_language: "es".into(),
        license_notice: "Original test fixture: CC0-1.0".into(),
    })
    .unwrap()
}

#[test]
fn separate_provider_preview_and_persistent_round_trip_without_anki_or_mexilex() {
    let adapter = adapter();
    assert_eq!(adapter.format_id(), "retivus-study-tsv-v1");
    let preview = validate_preview(&adapter, &mut Cursor::new(TSV)).unwrap();
    assert_eq!(preview.validated_count, 3);
    assert_eq!(preview.rows.len(), 3);
    assert_eq!(preview.rows[0].provenance.lexical_language, "ja");
    assert_eq!(preview.rows[0].provenance.definition_language, "es");
    assert_eq!(preview.rows[0].senses[0].definition, "vida");

    let root = tempdir().unwrap();
    let location = root.path().join("independent.sqlite3");
    let mut catalog = SourceCatalog::create_new(&location).unwrap();
    assert_eq!(
        catalog
            .import_snapshot("ja-es-v1", &adapter, &mut Cursor::new(TSV))
            .unwrap(),
        3
    );
    assert_eq!(
        catalog.counts("independent-ja-es", "ja-es-v1").unwrap(),
        (3, 3)
    );
    drop(catalog);
    let saved = SourceCatalog::open_existing(&location).unwrap();
    let first = saved
        .lookup_entry("independent-ja-es", "ja-es-v1", "ja1")
        .unwrap()
        .unwrap();
    let second = saved
        .lookup_entry("independent-ja-es", "ja-es-v1", "ja2")
        .unwrap()
        .unwrap();
    assert_eq!(first.word, second.word);
    assert_ne!(first.entry_id, second.entry_id);
    assert_eq!(first.senses[0].definition, "vida");
    assert_eq!(second.senses[0].definition, "vivir");
    assert_eq!(
        first.provenance.license_notice,
        adapter.descriptor().license_notice
    );
    assert!(saved
        .lookup_entry("mexilex", "ja-es-v1", "ja1")
        .unwrap()
        .is_none());
    assert!(saved
        .lookup_entry("independent-ja-es", "other", "ja1")
        .unwrap()
        .is_none());
}
