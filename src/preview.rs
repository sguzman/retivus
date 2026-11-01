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
}
