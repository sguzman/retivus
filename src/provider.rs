//! Stateless provider import boundary. This validates data but does not mutate a learner profile.
use serde_json::Value;
use std::collections::HashSet;
use std::io::BufRead;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub source: String,
    pub edition: String,
    pub entry_id: String,
    pub word: String,
    pub part_of_speech: String,
    pub sense_ids: Vec<String>,
    pub definitions_en: Vec<String>,
}

pub fn read_mexilex<R: BufRead>(reader: R) -> Result<Vec<Record>, String> {
    let mut ids = HashSet::new();
    let mut result = Vec::new();
    for (number, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| format!("line {}: {e}", number + 1))?;
        if line.trim().is_empty() {
            return Err(format!("line {}: empty record", number + 1));
        }
        let row: Value = serde_json::from_str(&line)
            .map_err(|e| format!("line {}: invalid JSON: {e}", number + 1))?;
        let str_field = |name: &str| {
            row.get(name)
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| format!("line {}: missing {name}", number + 1))
        };
        if row.get("format_version").and_then(Value::as_u64) != Some(1)
            || str_field("source")? != "mexilex"
            || str_field("lexical_language")? != "es-MX"
            || str_field("definition_language")? != "en"
        {
            return Err(format!(
                "line {}: unsupported provider contract",
                number + 1
            ));
        }
        let entry_id = str_field("entry_id")?.to_owned();
        if !ids.insert(entry_id.clone()) {
            return Err(format!("line {}: duplicate lexical entry ID", number + 1));
        }
        let senses = row
            .get("senses")
            .and_then(Value::as_array)
            .filter(|a| !a.is_empty())
            .ok_or_else(|| format!("line {}: missing senses", number + 1))?;
        let mut sense_ids = Vec::new();
        let mut definitions_en = Vec::new();
        let mut seen_senses = HashSet::new();
        for sense in senses {
            let id = sense
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("line {}: missing sense ID", number + 1))?;
            let definition = sense
                .get("definition_en")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| format!("line {}: missing English meaning", number + 1))?;
            if !seen_senses.insert(id.to_owned()) {
                return Err(format!("line {}: duplicate sense ID", number + 1));
            }
            sense_ids.push(id.to_owned());
            definitions_en.push(definition.to_owned());
        }
        result.push(Record {
            source: "mexilex".into(),
            edition: str_field("source_edition")?.to_owned(),
            entry_id,
            word: str_field("word")?.to_owned(),
            part_of_speech: str_field("part_of_speech")?.to_owned(),
            sense_ids,
            definitions_en,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn row(id: &str) -> String {
        serde_json::json!({
            "format_version":1, "source":"mexilex", "source_edition":"enwiktionary",
            "lexical_language":"es-MX", "definition_language":"en",
            "entry_id":id, "word":"banqueta", "part_of_speech":"noun",
            "senses":[{"id":"s1","definition_en":"sidewalk"},{"id":"s2","definition_en":"footstool"}]
        }).to_string()
    }
    #[test]
    fn keeps_multiple_meanings_without_creating_cards() {
        let result = read_mexilex(Cursor::new(row("a"))).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].definitions_en, ["sidewalk", "footstool"]);
    }
    #[test]
    fn refuses_duplicate_entries_and_duplicate_senses() {
        let duplicate = format!("{}\n{}\n", row("a"), row("a"));
        assert!(read_mexilex(Cursor::new(duplicate))
            .unwrap_err()
            .contains("duplicate lexical"));
        let mut bad: Value = serde_json::from_str(&row("a")).unwrap();
        bad["senses"][1]["id"] = serde_json::json!("s1");
        assert!(read_mexilex(Cursor::new(bad.to_string()))
            .unwrap_err()
            .contains("duplicate sense"));
    }
    #[test]
    fn rejects_incompatible_contracts_and_blank_meanings() {
        let mut bad: Value = serde_json::from_str(&row("a")).unwrap();
        bad["definition_language"] = serde_json::json!("es");
        assert!(read_mexilex(Cursor::new(bad.to_string())).is_err());
        bad["definition_language"] = serde_json::json!("en");
        bad["senses"][0]["definition_en"] = serde_json::json!("");
        assert!(read_mexilex(Cursor::new(bad.to_string())).is_err());
    }
}
