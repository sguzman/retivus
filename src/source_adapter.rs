//! Provider-neutral source records and two explicit, versioned read-only adapters.
//!
//! This module never creates Anki cards or changes learner-owned state.
use crate::provider;
use std::collections::HashSet;
use std::io::BufRead;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    pub provider_id: String,
    pub edition: String,
    pub lexical_language: String,
    pub definition_language: String,
    pub license_notice: String,
}

impl SourceInfo {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("provider_id", &self.provider_id),
            ("edition", &self.edition),
            ("lexical_language", &self.lexical_language),
            ("definition_language", &self.definition_language),
            ("license_notice", &self.license_notice),
        ] {
            if value.trim().is_empty() {
                return Err(format!("missing provider descriptor field: {name}"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMeaning {
    pub sense_id: String,
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRecord {
    pub provenance: SourceInfo,
    pub entry_id: String,
    pub word: String,
    pub part_of_speech: String,
    pub senses: Vec<SourceMeaning>,
}

/// A source adapter validates and visits records without taking ownership of a learner collection.
/// The callback is a transient preview/staging sink, NOT a committing database transaction.
pub trait SourceAdapter {
    fn descriptor(&self) -> &SourceInfo;
    fn format_id(&self) -> &'static str;
    fn visit(
        &self,
        reader: &mut dyn BufRead,
        sink: &mut dyn FnMut(SourceRecord),
    ) -> Result<usize, String>;
}

pub struct MexilexV1 {
    info: SourceInfo,
}

impl Default for MexilexV1 {
    fn default() -> Self {
        Self {
            info: SourceInfo {
                provider_id: "mexilex".into(),
                edition: "enwiktionary".into(),
                lexical_language: "es-MX".into(),
                definition_language: "en".into(),
                license_notice: "Refer to Mexilex CORPUS-LICENSE.md and release sidecar notices".into(),
            },
        }
    }
}

impl SourceAdapter for MexilexV1 {
    fn descriptor(&self) -> &SourceInfo {
        &self.info
    }

    fn format_id(&self) -> &'static str {
        "mexilex-study-jsonl-v1"
    }

    fn visit(
        &self,
        reader: &mut dyn BufRead,
        sink: &mut dyn FnMut(SourceRecord),
    ) -> Result<usize, String> {
        self.info.validate()?;
        provider::visit_mexilex(reader, |record| {
            let senses = record
                .sense_ids
                .into_iter()
                .zip(record.definitions_en)
                .map(|(sense_id, definition)| SourceMeaning {
                    sense_id,
                    definition,
                })
                .collect();
            let mut provenance = self.info.clone();
            provenance.edition = record.edition;
            sink(SourceRecord {
                provenance,
                entry_id: record.entry_id,
                word: record.word,
                part_of_speech: record.part_of_speech,
                senses,
            });
        })
    }
}

/// A deliberately distinct format: one source-identified entry and sense per TSV line.
/// A repeated word is legal if its source entry ID is different; duplicate IDs are not.
/// The caller supplies explicit source/version/language/license metadata, not a filename guess.
pub struct StudyTsvV1 {
    info: SourceInfo,
}

impl StudyTsvV1 {
    pub fn new(info: SourceInfo) -> Result<Self, String> {
        info.validate()?;
        if info.provider_id == "mexilex" {
            return Err("study TSV provider_id cannot impersonate Mexilex".into());
        }
        Ok(Self { info })
    }
}

impl SourceAdapter for StudyTsvV1 {
    fn descriptor(&self) -> &SourceInfo {
        &self.info
    }

    fn format_id(&self) -> &'static str {
        "retivus-study-tsv-v1"
    }

    fn visit(
        &self,
        reader: &mut dyn BufRead,
        sink: &mut dyn FnMut(SourceRecord),
    ) -> Result<usize, String> {
        self.info.validate()?;
        let mut lines = reader.lines();
        let header = lines
            .next()
            .ok_or("line 1: expected TSV header")?
            .map_err(|error| format!("line 1: {error}"))?;
        if header != "entry_id\tword\tpart_of_speech\tsense_id\tdefinition" {
            return Err("line 1: unsupported TSV format/version/header".into());
        }

        let mut seen_ids = HashSet::new();
        let mut count = 0;
        for (offset, line) in lines.enumerate() {
            let line_no = offset + 2;
            let line = line.map_err(|error| format!("line {line_no}: {error}"))?;
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() != 5 || fields.iter().any(|s| s.trim().is_empty()) {
                return Err(format!("line {line_no}: expected five nonempty TSV fields"));
            }
            if !seen_ids.insert(fields[0].to_owned()) {
                return Err(format!("line {line_no}: duplicate provider entry ID"));
            }
            sink(SourceRecord {
                provenance: self.info.clone(),
                entry_id: fields[0].into(),
                word: fields[1].into(),
                part_of_speech: fields[2].into(),
                senses: vec![SourceMeaning {
                    sense_id: fields[3].into(),
                    definition: fields[4].into(),
                }],
            });
            count += 1;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn independent_info() -> SourceInfo {
        SourceInfo {
            provider_id: "community-vocabulary".into(),
            edition: "2026-11-01".into(),
            lexical_language: "ja".into(),
            definition_language: "es".into(),
            license_notice: "CC0 sample, supplied by the source owner".into(),
        }
    }

    fn collect(adapter: &dyn SourceAdapter, data: &str) -> Result<Vec<SourceRecord>, String> {
        let mut result = Vec::new();
        adapter.visit(&mut Cursor::new(data), &mut |record| result.push(record))?;
        Ok(result)
    }

    #[test]
    fn independent_formats_share_a_source_neutral_contract() {
        let mex = r#"{"format_version":1,"source":"mexilex","source_edition":"enwiktionary","lexical_language":"es-MX","definition_language":"en","entry_id":"mx1","word":"banqueta","part_of_speech":"noun","senses":[{"id":"s1","definition_en":"sidewalk"}]}"#;
        let m: &dyn SourceAdapter = &MexilexV1::default();
        let t: &dyn SourceAdapter = &StudyTsvV1::new(independent_info()).unwrap();
        let mex_rows = collect(m, mex).unwrap();
        let tsv_rows = collect(t, "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\nja1\t猫\tnoun\tja1.s1\tgato\n").unwrap();
        assert_eq!(mex_rows[0].senses[0].definition, "sidewalk");
        assert_eq!(tsv_rows[0].senses[0].definition, "gato");
        assert_eq!(tsv_rows[0].provenance.lexical_language, "ja");
        assert_eq!(tsv_rows[0].provenance.definition_language, "es");
        assert_ne!(m.format_id(), t.format_id());
        assert_ne!(mex_rows[0].provenance.provider_id, tsv_rows[0].provenance.provider_id);
    }

    #[test]
    fn distinct_tsv_provider_rejects_incompatible_or_ambiguous_input() {
        let adapter = StudyTsvV1::new(independent_info()).unwrap();
        assert!(collect(&adapter, "entry_id,word,part_of_speech,sense_id,definition").is_err());
        let data = "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\na\t猫\tnoun\t1\tgato\na\t犬\tnoun\t2\tperro\n";
        assert!(collect(&adapter, data).unwrap_err().contains("duplicate"));
        let incomplete = "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\na\t猫\tnoun\t1\t\n";
        assert!(collect(&adapter, incomplete).is_err());
        let mut info = independent_info();
        info.license_notice.clear();
        assert!(StudyTsvV1::new(info).is_err());
        let mut info = independent_info();
        info.provider_id = "mexilex".into();
        assert!(StudyTsvV1::new(info).is_err());
    }
}
