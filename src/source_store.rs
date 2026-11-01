//! Transactional provider source catalog. This contains published source content,
//! never learner-owned notes, cards, ratings or review history.
use crate::source_adapter::{SourceAdapter, SourceInfo, SourceMeaning, SourceRecord};
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;
use std::io::BufRead;
use std::path::Path;

/// A single explicitly selected source sense, not a learner note or review event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCardCandidate {
    pub provenance: SourceInfo,
    pub snapshot: String,
    pub entry_id: String,
    pub sense_id: String,
    pub word: String,
    pub part_of_speech: String,
    pub definition: String,
}

impl SourceCardCandidate {
    /// Reversible source identity in an Anki-safe hierarchical tag. No private
    /// review state is encoded, and UTF-8 components cannot collide on delimiters.
    pub fn source_tag(&self) -> String {
        let mut tag = String::from("retivus::source");
        for component in [
            &self.provenance.provider_id,
            &self.snapshot,
            &self.entry_id,
            &self.sense_id,
        ] {
            tag.push_str("::");
            for byte in component.as_bytes() {
                tag.push_str(&format!("{byte:02x}"));
            }
        }
        tag
    }

    /// Reverse a Retivus provenance tag without guessing at word spellings.
    /// Reject malformed, truncated, non-UTF-8 and foreign tag namespaces.
    pub fn parse_source_tag(tag: &str) -> Result<[String; 4], String> {
        let payload = tag
            .strip_prefix("retivus::source::")
            .ok_or("not a Retivus source-identity tag")?;
        let mut parts = payload.split("::");
        fn decode(part: &str) -> Result<String, String> {
            if part.is_empty()
                || part.len() % 2 != 0
                || !part.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("invalid source-identity hex component".into());
            }
            let mut bytes = Vec::with_capacity(part.len() / 2);
            for pair in part.as_bytes().chunks_exact(2) {
                let hex = std::str::from_utf8(pair).map_err(|e| e.to_string())?;
                bytes.push(u8::from_str_radix(hex, 16).map_err(|e| e.to_string())?);
            }
            String::from_utf8(bytes).map_err(|e| e.to_string())
        }
        let mut next = || {
            let part = parts.next().ok_or("truncated source-identity tag")?;
            decode(part)
        };
        let provider = next()?;
        let snapshot = next()?;
        let entry = next()?;
        let sense = next()?;
        if parts.next().is_some() {
            return Err("source-identity tag has extra components".into());
        }
        Ok([provider, snapshot, entry, sense])
    }
}

pub struct SourceCatalog {
    connection: Connection,
}

impl SourceCatalog {
    pub fn in_memory() -> Result<Self, String> {
        Self::from_connection(Connection::open_in_memory().map_err(|e| e.to_string())?)
    }

    /// Create a brand-new catalog. Never silently overwrite an existing database.
    pub fn create_new(path: &Path) -> Result<Self, String> {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        Self::from_connection(Connection::open(path).map_err(|e| e.to_string())?)
    }

    pub fn open_existing(path: &Path) -> Result<Self, String> {
        let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !meta.file_type().is_file() || meta.file_type().is_symlink() {
            return Err("catalog must be a regular file, not a symlink".into());
        }
        Self::from_connection(Connection::open(path).map_err(|e| e.to_string())?)
    }

    fn from_connection(connection: Connection) -> Result<Self, String> {
        connection
            .execute_batch(
                "PRAGMA foreign_keys=ON;
                 CREATE TABLE IF NOT EXISTS snapshots (
                   provider TEXT NOT NULL,
                   snapshot TEXT NOT NULL,
                   edition TEXT NOT NULL,
                   lexical_language TEXT NOT NULL,
                   definition_language TEXT NOT NULL,
                   license_notice TEXT NOT NULL,
                   PRIMARY KEY(provider, snapshot)
                 );
                 CREATE TABLE IF NOT EXISTS entries (
                   provider TEXT NOT NULL,
                   snapshot TEXT NOT NULL,
                   entry_id TEXT NOT NULL,
                   source_edition TEXT NOT NULL,
                   word TEXT NOT NULL,
                   part_of_speech TEXT NOT NULL,
                   PRIMARY KEY(provider, snapshot, entry_id),
                   FOREIGN KEY(provider, snapshot) REFERENCES snapshots(provider, snapshot)
                 );
                 CREATE TABLE IF NOT EXISTS senses (
                   provider TEXT NOT NULL,
                   snapshot TEXT NOT NULL,
                   entry_id TEXT NOT NULL,
                   sense_id TEXT NOT NULL,
                   definition TEXT NOT NULL,
                   PRIMARY KEY(provider, snapshot, entry_id, sense_id),
                   FOREIGN KEY(provider, snapshot, entry_id)
                     REFERENCES entries(provider, snapshot, entry_id)
                 );",
            )
            .map_err(|e| e.to_string())?;
        Ok(Self { connection })
    }

    /// Stage an entire snapshot in one SQLite transaction. Any adapter or database
    /// error rolls back all of its records; existing snapshots remain untouched.
    /// Caller must verify archive checksums/licenses *before* invoking this.
    pub fn import_snapshot(
        &mut self,
        snapshot: &str,
        adapter: &dyn SourceAdapter,
        reader: &mut dyn BufRead,
    ) -> Result<usize, String> {
        if snapshot.trim().is_empty() || snapshot.len() > 128 {
            return Err("snapshot identifier must be nonempty and at most 128 bytes".into());
        }
        let info = adapter.descriptor();
        info.validate()?;
        let tx = self.connection.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO snapshots
              (provider, snapshot, edition, lexical_language, definition_language, license_notice)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                info.provider_id,
                snapshot,
                info.edition,
                info.lexical_language,
                info.definition_language,
                info.license_notice
            ],
        )
        .map_err(|e| format!("snapshot already exists or metadata rejected: {e}"))?;

        let mut insert_failure: Option<String> = None;
        let result = {
            let mut entries = tx
                .prepare(
                    "INSERT INTO entries
                      (provider, snapshot, entry_id, source_edition, word, part_of_speech)
                      VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(|e| e.to_string())?;
            let mut senses = tx
                .prepare(
                    "INSERT INTO senses (provider, snapshot, entry_id, sense_id, definition)
                      VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(|e| e.to_string())?;
            adapter.visit(reader, &mut |record: SourceRecord| {
                if insert_failure.is_some() {
                    return;
                }
                if record.provenance.provider_id != info.provider_id
                    || record.provenance.lexical_language != info.lexical_language
                    || record.provenance.definition_language != info.definition_language
                {
                    insert_failure =
                        Some("provider changed its declared identity or languages".into());
                    return;
                }
                if let Err(error) = entries.execute(params![
                    info.provider_id,
                    snapshot,
                    record.entry_id,
                    record.provenance.edition,
                    record.word,
                    record.part_of_speech
                ]) {
                    insert_failure = Some(format!("failed to insert source entry: {error}"));
                    return;
                }
                for sense in record.senses {
                    if let Err(error) = senses.execute(params![
                        info.provider_id,
                        snapshot,
                        record.entry_id,
                        sense.sense_id,
                        sense.definition
                    ]) {
                        insert_failure = Some(format!("failed to insert source sense: {error}"));
                        return;
                    }
                }
            })
        };
        let count = result?;
        if let Some(error) = insert_failure {
            return Err(error);
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(count)
    }

    /// Fetch one entry by immutable provider/snapshot/entry identity.
    /// Homographs and senses remain separate; no learner-owned data is queried.
    pub fn lookup_entry(
        &self,
        provider: &str,
        snapshot: &str,
        entry_id: &str,
    ) -> Result<Option<SourceRecord>, String> {
        let found: Option<(String, String, String, String, String, String)> = self
            .connection
            .query_row(
                "SELECT e.source_edition, e.word, e.part_of_speech,
                        s.lexical_language, s.definition_language, s.license_notice
                 FROM entries e JOIN snapshots s
                   ON s.provider = e.provider AND s.snapshot = e.snapshot
                 WHERE e.provider=?1 AND e.snapshot=?2 AND e.entry_id=?3",
                params![provider, snapshot, entry_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some((
            edition,
            word,
            part_of_speech,
            lexical_language,
            definition_language,
            license_notice,
        )) = found
        else {
            return Ok(None);
        };

        let mut stmt = self
            .connection
            .prepare(
                "SELECT sense_id, definition FROM senses
                 WHERE provider=?1 AND snapshot=?2 AND entry_id=?3
                 ORDER BY sense_id",
            )
            .map_err(|e| e.to_string())?;
        let senses = stmt
            .query_map(params![provider, snapshot, entry_id], |row| {
                Ok(SourceMeaning {
                    sense_id: row.get(0)?,
                    definition: row.get(1)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(Some(SourceRecord {
            provenance: SourceInfo {
                provider_id: provider.to_owned(),
                edition,
                lexical_language,
                definition_language,
                license_notice,
            },
            entry_id: entry_id.to_owned(),
            word,
            part_of_speech,
            senses,
        }))
    }

    /// Resolve one explicit sense before a learner chooses to create a card.
    /// Never fall back to a similarly spelled entry or a different source edition.
    pub fn select_sense(
        &self,
        provider: &str,
        snapshot: &str,
        entry_id: &str,
        sense_id: &str,
    ) -> Result<SourceCardCandidate, String> {
        let record = self
            .lookup_entry(provider, snapshot, entry_id)?
            .ok_or("source entry does not exist in requested snapshot")?;
        let definition = record
            .senses
            .iter()
            .find(|sense| sense.sense_id == sense_id)
            .ok_or("source sense does not exist for requested entry")?
            .definition
            .clone();
        Ok(SourceCardCandidate {
            provenance: record.provenance,
            snapshot: snapshot.to_owned(),
            entry_id: record.entry_id,
            sense_id: sense_id.to_owned(),
            word: record.word,
            part_of_speech: record.part_of_speech,
            definition,
        })
    }

    pub fn counts(&self, provider: &str, snapshot: &str) -> Result<(i64, i64), String> {
        let entries = self
            .connection
            .query_row(
                "SELECT count(*) FROM entries WHERE provider=?1 AND snapshot=?2",
                params![provider, snapshot],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let senses = self
            .connection
            .query_row(
                "SELECT count(*) FROM senses WHERE provider=?1 AND snapshot=?2",
                params![provider, snapshot],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        Ok((entries, senses))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_adapter::{MexilexV1, SourceInfo, StudyTsvV1};
    use std::io::Cursor;
    use tempfile::tempdir;

    fn tsv() -> StudyTsvV1 {
        StudyTsvV1::new(SourceInfo {
            provider_id: "other-dictionary".into(),
            edition: "2026-11".into(),
            lexical_language: "ja".into(),
            definition_language: "en".into(),
            license_notice: "Owner-provided CC0 test words".into(),
        })
        .unwrap()
    }

    const HEADER: &str = "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n";

    #[test]
    fn imports_multiple_independent_providers_and_retains_homograph_ids() {
        let mut catalog = SourceCatalog::in_memory().unwrap();
        let tsv_data = format!("{HEADER}a\t橋\tnoun\ts1\tbridge\nb\t橋\tverb\ts2\tto connect\n");
        let n = catalog
            .import_snapshot("v1", &tsv(), &mut Cursor::new(tsv_data))
            .unwrap();
        assert_eq!(n, 2);
        assert_eq!(catalog.counts("other-dictionary", "v1").unwrap(), (2, 2));

        let mex = r#"{"format_version":1,"source":"mexilex","source_edition":"enwiktionary","lexical_language":"es-MX","definition_language":"en","entry_id":"mx1","word":"banqueta","part_of_speech":"noun","senses":[{"id":"s1","definition_en":"sidewalk"},{"id":"s2","definition_en":"footstool"}]}"#;
        assert_eq!(
            catalog
                .import_snapshot("v0.3.0", &MexilexV1::default(), &mut Cursor::new(mex))
                .unwrap(),
            1
        );
        assert_eq!(catalog.counts("mexilex", "v0.3.0").unwrap(), (1, 2));
        assert_eq!(catalog.counts("other-dictionary", "v1").unwrap(), (2, 2));
        let first = catalog
            .select_sense("mexilex", "v0.3.0", "mx1", "s1")
            .unwrap();
        let second = catalog
            .select_sense("mexilex", "v0.3.0", "mx1", "s2")
            .unwrap();
        assert_eq!(first.word, second.word);
        assert_eq!(first.definition, "sidewalk");
        assert_eq!(second.definition, "footstool");
        assert_ne!(first.source_tag(), second.source_tag());
    }

    #[test]
    fn distinct_provider_preview_import_and_exact_sense_lookup() {
        use crate::preview::validate_preview;

        let dir = tempdir().unwrap();
        let path = dir.path().join("other.sqlite3");
        let adapter = tsv();
        let data = format!("{HEADER}ja1\t生\tverb\ts1\tto live\nja2\t生\tnoun\ts1\tlife\n");
        let preview = validate_preview(&adapter, &mut Cursor::new(&data)).unwrap();
        assert_eq!(preview.validated_count, 2);
        assert_eq!(preview.rows[0].entry_id, "ja1");
        assert_eq!(preview.rows[1].entry_id, "ja2");

        let mut catalog = SourceCatalog::create_new(&path).unwrap();
        assert_eq!(
            catalog
                .import_snapshot("v1", &adapter, &mut Cursor::new(data))
                .unwrap(),
            2
        );
        drop(catalog);
        let catalog = SourceCatalog::open_existing(&path).unwrap();
        let first = catalog
            .lookup_entry("other-dictionary", "v1", "ja1")
            .unwrap()
            .unwrap();
        let second = catalog
            .lookup_entry("other-dictionary", "v1", "ja2")
            .unwrap()
            .unwrap();
        assert_eq!(first.word, second.word);
        assert_ne!(first.entry_id, second.entry_id);
        assert_eq!(first.senses[0].definition, "to live");
        assert_eq!(second.senses[0].definition, "life");
        assert_eq!(first.provenance.lexical_language, "ja");
        assert_eq!(
            first.provenance.license_notice,
            adapter.descriptor().license_notice
        );
        assert!(catalog
            .lookup_entry("mexilex", "v1", "ja1")
            .unwrap()
            .is_none());
        assert!(catalog
            .lookup_entry("other-dictionary", "v2", "ja1")
            .unwrap()
            .is_none());
        assert!(catalog
            .lookup_entry("other-dictionary", "v1", "missing")
            .unwrap()
            .is_none());
        let a = catalog
            .select_sense("other-dictionary", "v1", "ja1", "s1")
            .unwrap();
        let b = catalog
            .select_sense("other-dictionary", "v1", "ja2", "s1")
            .unwrap();
        assert_eq!(a.word, b.word);
        assert_ne!(a.definition, b.definition);
        assert_ne!(a.source_tag(), b.source_tag());
        assert!(a.source_tag().starts_with("retivus::source::"));
        assert!(catalog
            .select_sense("other-dictionary", "v1", "ja1", "other")
            .is_err());
        assert!(catalog.select_sense("mexilex", "v1", "ja1", "s1").is_err());
    }

    #[test]
    fn provenance_tag_round_trips_unicode_and_rejects_ambiguity() {
        let identity = [
            "source::日本語".to_owned(),
            "2026-秋".to_owned(),
            "niño/猫".to_owned(),
            "meaning::1".to_owned(),
        ];
        let candidate = SourceCardCandidate {
            provenance: SourceInfo {
                provider_id: identity[0].clone(),
                edition: "test".into(),
                lexical_language: "ja".into(),
                definition_language: "es".into(),
                license_notice: "Synthetic fixture".into(),
            },
            snapshot: identity[1].clone(),
            entry_id: identity[2].clone(),
            sense_id: identity[3].clone(),
            word: "猫".into(),
            part_of_speech: "noun".into(),
            definition: "gato".into(),
        };
        let tag = candidate.source_tag();
        assert_eq!(SourceCardCandidate::parse_source_tag(&tag).unwrap(), identity);
        assert!(SourceCardCandidate::parse_source_tag("foreign::source::aaaa").is_err());
        assert!(SourceCardCandidate::parse_source_tag("retivus::source::61").is_err());
        assert!(SourceCardCandidate::parse_source_tag("retivus::source::6g::61::61::61").is_err());
        assert!(SourceCardCandidate::parse_source_tag("retivus::source::ff::61::61::61").is_err());
        assert!(SourceCardCandidate::parse_source_tag(
            "retivus::source::61::61::61::61::61"
        )
        .is_err());
    }

    #[test]
    fn late_invalid_input_rolls_back_entire_new_snapshot() {
        let mut catalog = SourceCatalog::in_memory().unwrap();
        let valid = format!("{HEADER}a\t橋\tnoun\ts1\tbridge\n");
        catalog
            .import_snapshot("original", &tsv(), &mut Cursor::new(valid))
            .unwrap();
        let invalid = format!(
            "{HEADER}a\t橋\tnoun\ts1\tbridge\nb\t犬\tnoun\ts2\tdog\nb\t猫\tnoun\ts3\tcat\n"
        );
        assert!(catalog
            .import_snapshot("broken", &tsv(), &mut Cursor::new(invalid))
            .is_err());
        assert_eq!(
            catalog.counts("other-dictionary", "broken").unwrap(),
            (0, 0)
        );
        assert_eq!(
            catalog.counts("other-dictionary", "original").unwrap(),
            (1, 1)
        );
    }

    #[test]
    fn duplicate_snapshot_refuses_overwrite_without_changing_rows() {
        let mut catalog = SourceCatalog::in_memory().unwrap();
        let data = format!("{HEADER}a\t犬\tnoun\ts1\tdog\n");
        catalog
            .import_snapshot("immutable", &tsv(), &mut Cursor::new(data.clone()))
            .unwrap();
        assert!(catalog
            .import_snapshot("immutable", &tsv(), &mut Cursor::new(data))
            .is_err());
        assert_eq!(
            catalog.counts("other-dictionary", "immutable").unwrap(),
            (1, 1)
        );
    }

    #[test]
    fn disk_catalog_reopens_and_cannot_replace_existing_source_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite3");
        let data = format!("{HEADER}a\t猫\tnoun\ts1\tcat\n");
        let mut catalog = SourceCatalog::create_new(&path).unwrap();
        catalog
            .import_snapshot("v1", &tsv(), &mut Cursor::new(data))
            .unwrap();
        drop(catalog);
        assert!(SourceCatalog::create_new(&path).is_err());
        assert_eq!(
            SourceCatalog::open_existing(&path)
                .unwrap()
                .counts("other-dictionary", "v1")
                .unwrap(),
            (1, 1)
        );
    }
}
