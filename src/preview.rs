//! Bounded, read-only source preview. Caller does not see partial results on validation failure.
use crate::source_adapter::{SourceAdapter, SourceRecord};
use std::io::BufRead;

pub const MAX_PREVIEW_ROWS: usize = 12;

#[derive(Debug)]
pub struct SourcePreview {
    pub validated_count: usize,
    pub rows: Vec<SourceRecord>,
}

pub fn validate_preview(
    adapter: &dyn SourceAdapter,
    reader: &mut dyn BufRead,
) -> Result<SourcePreview, String> {
    let mut rows = Vec::new();
    let validated_count = adapter.visit(reader, &mut |record| {
        if rows.len() < MAX_PREVIEW_ROWS {
            rows.push(record);
        }
    })?;
    Ok(SourcePreview {
        validated_count,
        rows,
    })
}

/// A bounded search of the *entire* source. This revalidates every entry,
/// including records after the first 12 matches. No learner data is modified.
#[derive(Debug)]
pub struct SourceSearch {
    pub validated_count: usize,
    pub matching_count: usize,
    pub rows: Vec<SourceRecord>,
}

/// Case-insensitive lexical/definition lookup without keeping a full catalog
/// in GUI memory. A later source error invalidates the entire search.
pub fn search_preview(
    adapter: &dyn SourceAdapter,
    reader: &mut dyn BufRead,
    query: &str,
) -> Result<SourceSearch, String> {
    let needle = query.trim();
    if needle.is_empty() || needle.chars().count() > 128 {
        return Err("search must contain 1–128 non-whitespace characters".into());
    }
    let needle = needle.to_lowercase();
    let mut rows = Vec::new();
    let mut matching_count = 0;
    let validated_count = adapter.visit(reader, &mut |record| {
        let matches = record.word.to_lowercase().contains(&needle)
            || record
                .senses
                .iter()
                .any(|sense| sense.definition.to_lowercase().contains(&needle));
        if matches {
            matching_count += 1;
            if rows.len() < MAX_PREVIEW_ROWS {
                rows.push(record);
            }
        }
    })?;
    Ok(SourceSearch {
        validated_count,
        matching_count,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_adapter::{SourceInfo, StudyTsvV1};
    use std::io::Cursor;

    fn adapter() -> StudyTsvV1 {
        StudyTsvV1::new(SourceInfo {
            provider_id: "fixture".into(),
            edition: "v1".into(),
            lexical_language: "ja".into(),
            definition_language: "en".into(),
            license_notice: "Test fixture".into(),
        })
        .unwrap()
    }

    #[test]
    fn preview_has_strict_bound_but_counts_every_valid_entry() {
        let mut data = "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n".to_owned();
        for i in 0..120 {
            data.push_str(&format!("{i}\tword{i}\tnoun\ts{i}\tmeaning{i}\n"));
        }
        let preview = validate_preview(&adapter(), &mut Cursor::new(data)).unwrap();
        assert_eq!(preview.validated_count, 120);
        assert_eq!(preview.rows.len(), MAX_PREVIEW_ROWS);
        assert_eq!(preview.rows[0].entry_id, "0");
        assert_eq!(preview.rows[11].entry_id, "11");
    }

    #[test]
    fn validation_failure_never_returns_partial_preview() {
        let data = "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n1\tgood\tnoun\t1\tvalid\n1\tbad\tnoun\t2\tduplicate\n";
        assert!(validate_preview(&adapter(), &mut Cursor::new(data)).is_err());
    }
    #[test]
    fn searching_all_entries_only_returns_twelve_and_keeps_exact_identities() {
        let mut data = "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n".to_owned();
        data.push_str("unmatched\tzorro\tnoun\ts1\tfox\n");
        for i in 0..35 {
            data.push_str(&format!("entry{i}\tBanqueta{i}\tnoun\ts{i}\tsidewalk\n"));
        }
        data.push_str("last\tcalle\tnoun\ts1\tStreet or sidewalk\n");
        let search = search_preview(&adapter(), &mut Cursor::new(data), "BANQUETA").unwrap();
        assert_eq!(search.validated_count, 37);
        assert_eq!(search.matching_count, 35);
        assert_eq!(search.rows.len(), MAX_PREVIEW_ROWS);
        assert_eq!(search.rows[0].entry_id, "entry0");
        assert_eq!(search.rows[11].entry_id, "entry11");
        assert_eq!(search.rows[11].senses[0].sense_id, "s11");
    }

    #[test]
    fn search_supports_unicode_words_and_definition_matches() {
        let data = "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n1\tÁrbol\tnoun\ts1\ttree\n2\tvida\tnoun\ts2\tLife\n3\tárbol\tnoun\ts3\twood\n";
        let names = search_preview(&adapter(), &mut Cursor::new(data), "ÁRBOL").unwrap();
        assert_eq!(names.matching_count, 2);
        assert_eq!(names.rows[0].entry_id, "1");
        assert_eq!(names.rows[1].entry_id, "3");
        let meanings = search_preview(&adapter(), &mut Cursor::new(data), "life").unwrap();
        assert_eq!(meanings.matching_count, 1);
        assert_eq!(meanings.rows[0].entry_id, "2");
    }

    #[test]
    fn search_refuses_blank_or_overlong_terms_and_late_invalid_records() {
        let valid =
            "entry_id\tword\tpart_of_speech\tsense_id\tdefinition\n1\tvida\tnoun\ts1\tlife\n";
        assert!(search_preview(&adapter(), &mut Cursor::new(valid), "   ").is_err());
        assert!(search_preview(&adapter(), &mut Cursor::new(valid), &"a".repeat(129)).is_err());
        let invalid = format!("{valid}1\tduplicate\tnoun\ts2\tlate error\n");
        assert!(search_preview(&adapter(), &mut Cursor::new(invalid), "vida").is_err());
    }
}
