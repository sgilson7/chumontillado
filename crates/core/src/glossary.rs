//! The glossary: `GLOSSARY.md` is the source of truth for what each word in
//! the tool means. The core parses it, so the page (which shows a definition
//! when a reader hovers a term) and the copy tests read it the same way.
//!
//! Format, one entry per term:
//!
//! ```text
//! ### Term
//!
//! One or two plain sentences saying what the term means. A number a rule
//! decides is written as a placeholder, {like_this}, that the core fills.
//!
//! Not: words, that, must, not, be, used, for, it
//! ```
//!
//! Text inside an HTML comment is ignored, so the template can carry an
//! example entry that does not count.

use serde::Serialize;

/// One glossary entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub term: String,
    pub definition: String,
    /// Words that must not be used for this term.
    pub not: Vec<String>,
}

/// Why a glossary cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlossaryError {
    NoDefinition(String),
    Duplicate(String),
}

impl GlossaryError {
    pub fn message(&self) -> String {
        match self {
            GlossaryError::NoDefinition(term) => {
                format!("The glossary entry \"{term}\" has no definition.")
            }
            GlossaryError::Duplicate(term) => {
                format!("The glossary defines \"{term}\" twice.")
            }
        }
    }
}

fn without_comments(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("<!--") {
        out.push_str(&rest[..open]);
        match rest[open..].find("-->") {
            Some(close) => rest = &rest[open + close + "-->".len()..],
            None => rest = "",
        }
    }
    out.push_str(rest);
    out
}

/// Parses the glossary. Entries keep the file's order.
pub fn parse(markdown: &str) -> Result<Vec<Entry>, GlossaryError> {
    let text = without_comments(markdown);
    let mut entries: Vec<Entry> = Vec::new();
    let mut current: Option<Entry> = None;
    let finish = |entry: Option<Entry>, entries: &mut Vec<Entry>| -> Result<(), GlossaryError> {
        let Some(e) = entry else {
            return Ok(());
        };
        if e.definition.is_empty() {
            return Err(GlossaryError::NoDefinition(e.term));
        }
        if entries.iter().any(|x| x.term.eq_ignore_ascii_case(&e.term)) {
            return Err(GlossaryError::Duplicate(e.term));
        }
        entries.push(e);
        Ok(())
    };
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(term) = line.strip_prefix("### ") {
            finish(current.take(), &mut entries)?;
            current = Some(Entry {
                term: term.trim().to_string(),
                definition: String::new(),
                not: Vec::new(),
            });
        } else if line.starts_with('#') {
            finish(current.take(), &mut entries)?;
        } else if let Some(entry) = current.as_mut() {
            if let Some(words) = line.strip_prefix("Not:") {
                entry.not = words
                    .split(',')
                    .map(|w| w.trim().to_string())
                    .filter(|w| !w.is_empty())
                    .collect();
            } else if !line.is_empty() {
                if !entry.definition.is_empty() {
                    entry.definition.push(' ');
                }
                entry.definition.push_str(line);
            }
        }
    }
    finish(current.take(), &mut entries)?;
    Ok(entries)
}

/// Every term the glossary defines more than once, with the lines of its
/// first and second heading (1-based), for the check's message (R6.13).
pub fn duplicates(markdown: &str) -> Vec<(String, usize, usize)> {
    let mut seen: Vec<(String, usize)> = Vec::new();
    let mut out = Vec::new();
    let mut in_comment = false;
    for (k, raw) in markdown.lines().enumerate() {
        let line = raw.trim();
        if line.contains("<!--") && !line.contains("-->") {
            in_comment = true;
        }
        if in_comment {
            if line.contains("-->") {
                in_comment = false;
            }
            continue;
        }
        if let Some(term) = line.strip_prefix("### ") {
            let term = term.trim().to_string();
            match seen.iter().find(|(t, _)| t.eq_ignore_ascii_case(&term)) {
                Some((_, first)) => out.push((term, *first, k + 1)),
                None => seen.push((term, k + 1)),
            }
        }
    }
    out
}

/// The glossary as the game loads it (M7, R6.13): a term defined twice keeps
/// its first entry, and an entry with no definition is left out, so a slip
/// in `GLOSSARY.md` costs a hover hint, not the game. The check
/// (`parse`, in `cargo test` over `GLOSSARY.md`) still fails on either.
pub fn load(markdown: &str) -> Vec<Entry> {
    let mut text = without_comments(markdown);
    loop {
        match parse(&text) {
            Ok(entries) => return entries,
            Err(e) => {
                let (GlossaryError::NoDefinition(term) | GlossaryError::Duplicate(term)) = &e;
                let dup = matches!(e, GlossaryError::Duplicate(_));
                text = drop_entry(&text, term, dup);
            }
        }
    }
}

/// The text with one entry for `term` taken out: its last one when `last`,
/// else its first.
fn drop_entry(text: &str, term: &str, last: bool) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let heads: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            l.trim()
                .strip_prefix("### ")
                .is_some_and(|t| t.trim().eq_ignore_ascii_case(term))
        })
        .map(|(k, _)| k)
        .collect();
    let Some(&at) = (if last { heads.last() } else { heads.first() }) else {
        return String::new();
    };
    let end = (at + 1..lines.len())
        .find(|k| lines[*k].trim().starts_with('#'))
        .unwrap_or(lines.len());
    lines[..at]
        .iter()
        .chain(&lines[end..])
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Glossary\n\nIntro text.\n\n### Spout\n\nWhere slush comes out.\nIt sits above the belt.\n\nNot: tap, nozzle\n\n### Line\n\nOne belt with its spouts.\n";

    #[test]
    fn entries_come_back_in_file_order_with_their_not_words() {
        let entries = parse(SAMPLE).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].term, "Spout");
        assert_eq!(
            entries[0].definition,
            "Where slush comes out. It sits above the belt."
        );
        assert_eq!(entries[0].not, vec!["tap", "nozzle"]);
        assert_eq!(entries[1].term, "Line");
        assert!(entries[1].not.is_empty());
    }

    #[test]
    fn an_entry_without_a_definition_is_refused_by_name() {
        let e = parse("### Spout\n\nNot: tap\n").unwrap_err();
        assert_eq!(e, GlossaryError::NoDefinition("Spout".into()));
        assert_eq!(e.message(), "The glossary entry \"Spout\" has no definition.");
    }

    /// R6.13: the game still loads a glossary with a term defined twice (the
    /// first entry kept) or an entry left empty (left out); the check names
    /// the duplicate and both its lines.
    #[test]
    fn a_term_defined_twice_still_loads_and_the_check_names_both_lines() {
        let text = "# G\n\n### Spout\n\nA.\n\n### Line\n\nL.\n\n### spout\n\nB.\n\n### Empty\n\nNot: x\n";
        assert!(parse(text).is_err());
        let loaded = load(text);
        let terms: Vec<&str> = loaded.iter().map(|e| e.term.as_str()).collect();
        assert_eq!(terms, ["Spout", "Line"]);
        assert_eq!(loaded[0].definition, "A.");
        assert_eq!(duplicates(text), vec![("spout".to_string(), 3, 11)]);
        assert!(
            duplicates("### A\n\nx\n<!--\n### A\n-->\n").is_empty(),
            "a comment does not count"
        );
    }

    #[test]
    fn a_term_defined_twice_is_refused() {
        let e = parse("### Spout\n\nA.\n\n### spout\n\nB.\n").unwrap_err();
        assert_eq!(e, GlossaryError::Duplicate("spout".into()));
    }

    #[test]
    fn an_entry_inside_a_comment_does_not_count() {
        let entries = parse("<!--\n### Example\n\nNot real.\n-->\n### Real\n\nYes.\n").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].term, "Real");
    }

    #[test]
    fn a_heading_of_another_level_ends_the_entry() {
        let entries = parse("### Spout\n\nWhere slush comes out.\n\n## Notes\n\nNot part of it.\n").unwrap();
        assert_eq!(entries[0].definition, "Where slush comes out.");
    }
}
