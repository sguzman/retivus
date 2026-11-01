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
                || !part.len().is_multiple_of(2)
                || !part.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("invalid source-identity hex component".into());
            }
            let mut bytes = Vec::with_capacity(part.len() / 2);
            for pair in part.as_bytes().as_chunks::<2>().0 {
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

/// One source sense's lexical payload; review history is never included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensePayload {
    pub word: String,
    pub part_of_speech: String,
    pub definition: String,
}

/// A changed, removed or newly added exact sense between immutable snapshots.
/// No equal senses are returned and no homographs are automatically matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotSenseChange {
    pub entry_id: String,
    pub sense_id: String,
    pub before: Option<SensePayload>,
    pub after: Option<SensePayload>,
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
                 );
                 CREATE TABLE IF NOT EXISTS reconciliations (
                   provider TEXT NOT NULL,
                   from_snapshot TEXT NOT NULL,
                   from_entry TEXT NOT NULL,
                   from_sense TEXT NOT NULL,
                   to_snapshot TEXT NOT NULL,
                   to_entry TEXT NOT NULL,
                   to_sense TEXT NOT NULL,
                   PRIMARY KEY(provider, from_snapshot, from_entry, from_sense),
                   UNIQUE(provider, to_snapshot, to_entry, to_sense),
                   FOREIGN KEY(provider, from_snapshot, from_entry, from_sense)
                     REFERENCES senses(provider, snapshot, entry_id, sense_id),
                   FOREIGN KEY(provider, to_snapshot, to_entry, to_sense)
                     REFERENCES senses(provider, snapshot, entry_id, sense_id)
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

    /// Compare only differing exact source senses. Returning at most 1000
    /// differences per call keeps large provider migrations bounded.
    /// Removed+added homographs are conflicts for the caller, not automatic matches.
    pub fn diff_snapshots(
        &self,
        provider: &str,
        before: &str,
        after: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SnapshotSenseChange>, String> {
        if before == after || limit == 0 || limit > 1_000 {
            return Err("different snapshots and a limit from 1 through 1000 are required".into());
        }
        for snapshot in [before, after] {
            let present: i64 = self
                .connection
                .query_row(
                    "SELECT count(*) FROM snapshots WHERE provider=?1 AND snapshot=?2",
                    params![provider, snapshot],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            if present != 1 {
                return Err("snapshot not present for requested provider".into());
            }
        }
        let limit = i64::try_from(limit).map_err(|e| e.to_string())?;
        let offset = i64::try_from(offset).map_err(|e| e.to_string())?;
        let mut query = self
            .connection
            .prepare(
                "WITH before_senses AS (
                   SELECT e.entry_id, s.sense_id, e.word, e.part_of_speech, s.definition
                   FROM entries e JOIN senses s
                     ON e.provider=s.provider AND e.snapshot=s.snapshot AND e.entry_id=s.entry_id
                   WHERE e.provider=?1 AND e.snapshot=?2
                 ),
                 after_senses AS (
                   SELECT e.entry_id, s.sense_id, e.word, e.part_of_speech, s.definition
                   FROM entries e JOIN senses s
                     ON e.provider=s.provider AND e.snapshot=s.snapshot AND e.entry_id=s.entry_id
                   WHERE e.provider=?1 AND e.snapshot=?3
                 )
                 SELECT b.entry_id, b.sense_id,
                        b.word, b.part_of_speech, b.definition,
                        a.word, a.part_of_speech, a.definition
                 FROM before_senses b LEFT JOIN after_senses a
                   ON b.entry_id=a.entry_id AND b.sense_id=a.sense_id
                 WHERE a.entry_id IS NULL OR b.word<>a.word
                   OR b.part_of_speech<>a.part_of_speech OR b.definition<>a.definition
                 UNION ALL
                 SELECT a.entry_id, a.sense_id,
                        NULL, NULL, NULL, a.word, a.part_of_speech, a.definition
                 FROM after_senses a LEFT JOIN before_senses b
                   ON a.entry_id=b.entry_id AND a.sense_id=b.sense_id
                 WHERE b.entry_id IS NULL
                 ORDER BY 1, 2 LIMIT ?4 OFFSET ?5",
            )
            .map_err(|e| e.to_string())?;
        let changes = query
            .query_map(params![provider, before, after, limit, offset], |row| {
                let before_word: Option<String> = row.get(2)?;
                let after_word: Option<String> = row.get(5)?;
                Ok(SnapshotSenseChange {
                    entry_id: row.get(0)?,
                    sense_id: row.get(1)?,
                    before: if let Some(word) = before_word {
                        Some(SensePayload {
                            word,
                            part_of_speech: row.get(3)?,
                            definition: row.get(4)?,
                        })
                    } else {
                        None
                    },
                    after: if let Some(word) = after_word {
                        Some(SensePayload {
                            word,
                            part_of_speech: row.get(6)?,
                            definition: row.get(7)?,
                        })
                    } else {
                        None
                    },
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(changes)
    }

    /// Record an explicit one-to-one source identity migration. It changes no
    /// private Anki notes, original tags, due dates, ratings or review history.
    /// The link is reversible and cannot merge two old senses into one target.
    pub fn record_reconciliation(
        &mut self,
        provider: &str,
        from_snapshot: &str,
        from_entry: &str,
        from_sense: &str,
        to_snapshot: &str,
        to_entry: &str,
        to_sense: &str,
    ) -> Result<(), String> {
        if from_snapshot == to_snapshot {
            return Err("a reconciliation must cross source snapshots".into());
        }
        let old = self.select_sense(provider, from_snapshot, from_entry, from_sense)?;
        let new = self.select_sense(provider, to_snapshot, to_entry, to_sense)?;
        if old.provenance.lexical_language != new.provenance.lexical_language
            || old.provenance.definition_language != new.provenance.definition_language
        {
            return Err("source languages differ; automatic reconciliation is prohibited".into());
        }
        self.connection
            .execute(
                "INSERT INTO reconciliations
                 (provider, from_snapshot, from_entry, from_sense,
                  to_snapshot, to_entry, to_sense)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    provider,
                    from_snapshot,
                    from_entry,
                    from_sense,
                    to_snapshot,
                    to_entry,
                    to_sense
                ],
            )
            .map_err(|e| format!("ambiguous, duplicate, or invalid reconciliation: {e}"))?;
        Ok(())
    }

    /// Undo only a previously approved mapping, never any learner review events.
    pub fn undo_reconciliation(
        &mut self,
        provider: &str,
        snapshot: &str,
        entry_id: &str,
        sense_id: &str,
    ) -> Result<(), String> {
        let deleted = self
            .connection
            .execute(
                "DELETE FROM reconciliations
                 WHERE provider=?1 AND from_snapshot=?2 AND from_entry=?3 AND from_sense=?4",
                params![provider, snapshot, entry_id, sense_id],
            )
            .map_err(|e| e.to_string())?;
        if deleted != 1 {
            return Err("requested reconciliation is not recorded".into());
        }
        Ok(())
    }

    /// Resolve only a previously explicit mapping. Unmapped senses keep their
    /// original immutable source identity, even when their word matches another.
    pub fn reconciled_sense(
        &self,
        provider: &str,
        snapshot: &str,
        entry_id: &str,
        sense_id: &str,
    ) -> Result<SourceCardCandidate, String> {
        let target: Option<(String, String, String)> = self
            .connection
            .query_row(
                "SELECT to_snapshot, to_entry, to_sense FROM reconciliations
                 WHERE provider=?1 AND from_snapshot=?2 AND from_entry=?3 AND from_sense=?4",
                params![provider, snapshot, entry_id, sense_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some((new_snapshot, new_entry, new_sense)) = target {
            self.select_sense(provider, &new_snapshot, &new_entry, &new_sense)
        } else {
            self.select_sense(provider, snapshot, entry_id, sense_id)
        }
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
        assert_eq!(
            SourceCardCandidate::parse_source_tag(&tag).unwrap(),
            identity
        );
        assert!(SourceCardCandidate::parse_source_tag("foreign::source::aaaa").is_err());
        assert!(SourceCardCandidate::parse_source_tag("retivus::source::61").is_err());
        assert!(SourceCardCandidate::parse_source_tag("retivus::source::6g::61::61::61").is_err());
        assert!(SourceCardCandidate::parse_source_tag("retivus::source::ff::61::61::61").is_err());
        assert!(
            SourceCardCandidate::parse_source_tag("retivus::source::61::61::61::61::61").is_err()
        );
    }

    #[test]
    fn explicit_snapshot_reconciliation_detects_changes_and_cannot_merge_homographs() {
        let old = format!(
            "{HEADER}a\t橋\tnoun\ts1\tbridge\nb\t橋\tverb\ts1\tconnect\nc\t猫\tnoun\ts1\tcat\nd\t水\tnoun\ts1\twater\n"
        );
        let updated = format!(
            "{HEADER}a\t橋\tnoun\ts1\tbridge\nb\t橋\tverb\ts1\tlink\nx\t猫\tnoun\ts1\tcat\ny\t水\tnoun\ts1\twater\n"
        );
        let mut catalog = SourceCatalog::in_memory().unwrap();
        catalog
            .import_snapshot("old", &tsv(), &mut Cursor::new(old))
            .unwrap();
        catalog
            .import_snapshot("new", &tsv(), &mut Cursor::new(updated))
            .unwrap();
        let changes = catalog
            .diff_snapshots("other-dictionary", "old", "new", 100, 0)
            .unwrap();
        assert_eq!(changes.len(), 5);
        assert!(changes.iter().any(|c| {
            c.entry_id == "b"
                && c.before.as_ref().unwrap().definition == "connect"
                && c.after.as_ref().unwrap().definition == "link"
        }));
        assert!(changes
            .iter()
            .any(|c| c.entry_id == "c" && c.after.is_none()));
        assert!(changes
            .iter()
            .any(|c| c.entry_id == "d" && c.after.is_none()));
        assert!(changes
            .iter()
            .any(|c| c.entry_id == "x" && c.before.is_none()));
        assert!(changes
            .iter()
            .any(|c| c.entry_id == "y" && c.before.is_none()));
        assert!(!changes.iter().any(|c| c.entry_id == "a"));
        assert_eq!(
            catalog
                .diff_snapshots("other-dictionary", "old", "new", 2, 0)
                .unwrap(),
            changes[..2]
        );
        assert_eq!(
            catalog
                .diff_snapshots("other-dictionary", "old", "new", 2, 2)
                .unwrap(),
            changes[2..4]
        );
        assert!(catalog
            .diff_snapshots("other-dictionary", "old", "unknown", 2, 0)
            .is_err());
        assert!(catalog
            .diff_snapshots("other-dictionary", "old", "old", 2, 0)
            .is_err());
        assert!(catalog
            .diff_snapshots("other-dictionary", "old", "new", 0, 0)
            .is_err());

        let old_cat = catalog
            .select_sense("other-dictionary", "old", "c", "s1")
            .unwrap();
        let new_cat = catalog
            .select_sense("other-dictionary", "new", "x", "s1")
            .unwrap();
        assert_ne!(old_cat.source_tag(), new_cat.source_tag());
        assert_eq!(
            catalog
                .reconciled_sense("other-dictionary", "old", "c", "s1")
                .unwrap()
                .source_tag(),
            old_cat.source_tag()
        );
        catalog
            .record_reconciliation("other-dictionary", "old", "c", "s1", "new", "x", "s1")
            .unwrap();
        assert_eq!(
            catalog
                .reconciled_sense("other-dictionary", "old", "c", "s1")
                .unwrap()
                .source_tag(),
            new_cat.source_tag()
        );
        assert!(catalog
            .record_reconciliation("other-dictionary", "old", "d", "s1", "new", "x", "s1")
            .is_err());
        assert!(catalog
            .record_reconciliation("other-dictionary", "old", "a", "s1", "new", "missing", "s1")
            .is_err());
        catalog
            .undo_reconciliation("other-dictionary", "old", "c", "s1")
            .unwrap();
        assert_eq!(
            catalog
                .reconciled_sense("other-dictionary", "old", "c", "s1")
                .unwrap()
                .source_tag(),
            old_cat.source_tag()
        );
        assert!(catalog
            .undo_reconciliation("other-dictionary", "old", "c", "s1")
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
