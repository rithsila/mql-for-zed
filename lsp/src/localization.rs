use crate::hover_config::PrimaryLanguage;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalizedDoc {
    pub en: Option<String>,
    pub km: Option<String>,
    pub unclassified: Option<String>,
    pub vibe: Option<String>,
    pub raw: String,
}

impl LocalizedDoc {
    /// Return the preferred single text representation (used for backward compatibility with completion).
    pub fn primary_text(&self) -> String {
        if let Some(en) = &self.en {
            return en.clone();
        }
        if let Some(km) = &self.km {
            return km.clone();
        }
        if let Some(unc) = &self.unclassified {
            return unc.clone();
        }
        if let Some(vibe) = &self.vibe {
            return vibe.clone();
        }
        self.raw.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.en.is_none()
            && self.km.is_none()
            && self.unclassified.is_none()
            && self.vibe.is_none()
            && self.raw.trim().is_empty()
    }
}

/// Checks if a character belongs to the Khmer Unicode script blocks.
pub fn is_khmer_char(c: char) -> bool {
    ('\u{1780}'..='\u{17FF}').contains(&c) || ('\u{19E0}'..='\u{19FF}').contains(&c)
}

/// Checks if a character is a Latin/English alphabetic character.
pub fn is_latin_char(c: char) -> bool {
    c.is_ascii_alphabetic()
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum TagState {
    None,
    En,
    Km,
}

/// Parse extracted comment lines into a structured `LocalizedDoc`.
/// Supports:
/// - `// @en ...` and `// @km ...` tags.
/// - Continuation semantics: lines following a tag continue that language block until another tag.
/// - Preserves paragraph breaks (empty comment lines).
/// - Conservative legacy classification when no tags exist:
///   - Pure Khmer text -> km
///   - Pure Latin/English text -> en
///   - Mixed or ambiguous text -> unclassified (requires explicit tags to avoid single-language leakage)
pub fn parse_comments(raw_lines: &[String]) -> LocalizedDoc {
    if raw_lines.is_empty() {
        return LocalizedDoc::default();
    }

    let raw = raw_lines.join("\n");

    let has_tags = raw_lines.iter().any(|line| {
        let t = line.trim();
        t.starts_with("@en") || t.starts_with("@km")
    });

    if has_tags {
        let mut en_lines: Vec<String> = Vec::new();
        let mut km_lines: Vec<String> = Vec::new();
        let mut unc_lines: Vec<String> = Vec::new();
        let mut state = TagState::None;

        for line in raw_lines {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("@en") {
                state = TagState::En;
                let content = rest.strip_prefix(':').unwrap_or(rest).trim();
                en_lines.push(content.to_string());
            } else if let Some(rest) = t.strip_prefix("@km") {
                state = TagState::Km;
                let content = rest.strip_prefix(':').unwrap_or(rest).trim();
                km_lines.push(content.to_string());
            } else {
                match state {
                    TagState::En => en_lines.push(t.to_string()),
                    TagState::Km => km_lines.push(t.to_string()),
                    TagState::None => {
                        if !t.is_empty() {
                            unc_lines.push(t.to_string());
                        }
                    }
                }
            }
        }

        let clean_block = |lines: Vec<String>| -> Option<String> {
            let joined = lines.join("\n").trim().to_string();
            if joined.is_empty() {
                None
            } else {
                Some(joined)
            }
        };

        return LocalizedDoc {
            en: clean_block(en_lines),
            km: clean_block(km_lines),
            unclassified: clean_block(unc_lines),
            vibe: None,
            raw,
        };
    }

    // No tags present: classify conservatively based on script analysis.
    let full_trimmed = raw_lines.join(" ").trim().to_string();
    if full_trimmed.is_empty() {
        return LocalizedDoc::default();
    }

    let has_khmer = full_trimmed.chars().any(is_khmer_char);
    let has_latin = full_trimmed.chars().any(is_latin_char);

    if has_khmer && !has_latin {
        // Pure Khmer legacy comment
        LocalizedDoc {
            en: None,
            km: Some(full_trimmed),
            unclassified: None,
            vibe: None,
            raw,
        }
    } else if has_latin && !has_khmer {
        // Pure English legacy comment
        LocalizedDoc {
            en: Some(full_trimmed),
            km: None,
            unclassified: None,
            vibe: None,
            raw,
        }
    } else {
        // Mixed English/Khmer or ambiguous script -> unclassified.
        // Will be displayed in bilingual mode under unclassified, but suppressed in single-language mode
        // with an explicit tagging notice to prevent leaking mixed prose.
        LocalizedDoc {
            en: None,
            km: None,
            unclassified: Some(full_trimmed),
            vibe: None,
            raw,
        }
    }
}

pub struct Catalog;

impl Catalog {
    pub fn language_heading(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => "English",
            PrimaryLanguage::Khmer => "ខ្មែរ",
        }
    }

    pub fn unavailable_doc(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => "Documentation is not available in English.",
            PrimaryLanguage::Khmer => "មិនទាន់មានការបកប្រែជាភាសាខ្មែរ",
        }
    }

    pub fn tagging_notice(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => {
                "Untagged documentation requires // @en tag to display in English-only mode."
            }
            PrimaryLanguage::Khmer => "ឯកសារដែលមិនទាន់ដាក់ស្លាក ទាមទារស្លាក // @km ដើម្បីបង្ហាញក្នុងទម្រង់ភាសាខ្មែរ។",
        }
    }

    pub fn unclassified_heading(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => "Original documentation (unclassified)",
            PrimaryLanguage::Khmer => "ឯកសារដើម (មិនទាន់ចាត់ថ្នាក់)",
        }
    }

    pub fn source_label(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => "Source",
            PrimaryLanguage::Khmer => "ប្រភព",
        }
    }

    pub fn documentation_label(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => "Documentation",
            PrimaryLanguage::Khmer => "ឯកសារយោង",
        }
    }

    pub fn bundled_reference(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => "bundled reference",
            PrimaryLanguage::Khmer => "បញ្ជីឯកសារយោងភ្ជាប់មកជាមួយ",
        }
    }

    pub fn source_comment(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => "source comment",
            PrimaryLanguage::Khmer => "ការអធិប្បាយក្នុងកូដប្រភព",
        }
    }

    pub fn ai_knowledge_label(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => "AI Workspace Knowledge (.mql/knowledge.json)",
            PrimaryLanguage::Khmer => "មូលដ្ឋានចំណេះដឹង AI (.mql/knowledge.json)",
        }
    }

    pub fn vibe_label(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => "Vibe Check (Trading Context)",
            PrimaryLanguage::Khmer => "បរិបទនៃការជួញដូរ (Vibe Check)",
        }
    }

    pub fn set_parameter_provenance(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => ".set parameter definition",
            PrimaryLanguage::Khmer => "និយមន័យប៉ារ៉ាម៉ែត្រ .set",
        }
    }

    pub fn type_label(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => "Type",
            PrimaryLanguage::Khmer => "ប្រភេទ",
        }
    }

    pub fn vibe_explainer_heading(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => "MQL5 Vibe Explainer",
            PrimaryLanguage::Khmer => "ការពន្យល់កូដ MQL5 (Vibe Explainer)",
        }
    }

    pub fn trader_gotcha_label(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => "Trader Gotcha / Risk Tip",
            PrimaryLanguage::Khmer => "ចំណុចគួរប្រយ័ត្នសម្រាប់ការជួញដូរ (Risk Tip)",
        }
    }

    pub fn default_label(lang: PrimaryLanguage) -> &'static str {
        match lang {
            PrimaryLanguage::English => "Default",
            PrimaryLanguage::Khmer => "តម្លៃដើម",
        }
    }

    pub fn kind_label(lang: PrimaryLanguage, kind_name: &str) -> &'static str {
        match (lang, kind_name) {
            (PrimaryLanguage::Khmer, "Function") => "អនុគមន៍ (Function)",
            (PrimaryLanguage::Khmer, "Class") => "ថ្នាក់ (Class)",
            (PrimaryLanguage::Khmer, "Enum") => "Enum",
            (PrimaryLanguage::Khmer, "Constant") => "តម្លៃថេរ (Constant)",
            (PrimaryLanguage::Khmer, "Macro") => "ម៉ាក្រូ (Macro)",
            (PrimaryLanguage::Khmer, "Input") => "ធាតុបញ្ចូល (Input)",
            (PrimaryLanguage::Khmer, "Condition") => "លក្ខខណ្ឌត្រួតពិនិត្យ (Condition Gate)",
            (PrimaryLanguage::Khmer, "Loop") => "រង្វិលជុំ (Loop Iteration)",
            (PrimaryLanguage::Khmer, "Return") => "សេចក្តីត្រឡប់តម្លៃ (Return Statement)",
            (PrimaryLanguage::Khmer, "Statement") => "កន្សោមជួញដូរ (Trading Statement)",
            (PrimaryLanguage::Khmer, _) => "និមិត្តសញ្ញា (Symbol)",
            _ => match kind_name {
                "Function" => "Function",
                "Class" => "Class",
                "Enum" => "Enum",
                "Constant" => "Constant",
                "Macro" => "Macro",
                "Input" => "Input",
                "Condition" => "Condition Gate",
                "Loop" => "Loop Iteration",
                "Return" => "Return Statement",
                "Statement" => "Trading Statement",
                _ => "Symbol",
            },
        }
    }

    pub fn declaration_index(lang: PrimaryLanguage, index: usize, total: usize) -> String {
        match lang {
            PrimaryLanguage::English => format!("Declaration {} of {}", index, total),
            PrimaryLanguage::Khmer => format!("សេចក្តីប្រកាសទី {} នៃ {}", index, total),
        }
    }

    pub fn additional_declarations(lang: PrimaryLanguage, count: usize) -> String {
        match lang {
            PrimaryLanguage::English => format!("(+{} additional declarations)", count),
            PrimaryLanguage::Khmer => format!("(+{} សេចក្តីប្រកាសបន្ថែមទៀត)", count),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tagged_bilingual_parsing() {
        let lines = vec![
            "@en Identifier for the recovery engine.".to_string(),
            "@km លេខសម្គាល់សម្រាប់ម៉ាស៊ីនស្តារការខាតបង់។".to_string(),
        ];
        let doc = parse_comments(&lines);
        assert_eq!(
            doc.en.as_deref(),
            Some("Identifier for the recovery engine.")
        );
        assert_eq!(doc.km.as_deref(), Some("លេខសម្គាល់សម្រាប់ម៉ាស៊ីនស្តារការខាតបង់។"));
        assert!(doc.unclassified.is_none());
    }

    #[test]
    fn test_tagged_continuation_and_paragraphs() {
        let lines = vec![
            "@en First line of English.".to_string(),
            "Second line of English.".to_string(),
            "".to_string(),
            "Second paragraph.".to_string(),
            "@km បន្ទាត់ទីមួយជាភាសាខ្មែរ។".to_string(),
            "បន្ទាត់ទីពីរជាភាសាខ្មែរ។".to_string(),
        ];
        let doc = parse_comments(&lines);
        assert_eq!(
            doc.en.as_deref(),
            Some("First line of English.\nSecond line of English.\n\nSecond paragraph.")
        );
        assert_eq!(
            doc.km.as_deref(),
            Some("បន្ទាត់ទីមួយជាភាសាខ្មែរ។\nបន្ទាត់ទីពីរជាភាសាខ្មែរ។")
        );
    }

    #[test]
    fn test_legacy_pure_english() {
        let lines =
            vec!["Calculates the profit of a trade operation in account currency.".to_string()];
        let doc = parse_comments(&lines);
        assert_eq!(
            doc.en.as_deref(),
            Some("Calculates the profit of a trade operation in account currency.")
        );
        assert!(doc.km.is_none());
        assert!(doc.unclassified.is_none());
    }

    #[test]
    fn test_legacy_pure_khmer() {
        let lines = vec!["ត្រឡប់តម្លៃដាច់ខាតនៃចំនួនមួយ។".to_string()];
        let doc = parse_comments(&lines);
        assert!(doc.en.is_none());
        assert_eq!(doc.km.as_deref(), Some("ត្រឡប់តម្លៃដាច់ខាតនៃចំនួនមួយ។"));
        assert!(doc.unclassified.is_none());
    }

    #[test]
    fn test_legacy_mixed_is_unclassified() {
        let lines = vec!["បើកការជួញដូរ Buy ថ្មីតាមរយៈ CTrade".to_string()];
        let doc = parse_comments(&lines);
        assert!(doc.en.is_none());
        assert!(doc.km.is_none());
        assert_eq!(
            doc.unclassified.as_deref(),
            Some("បើកការជួញដូរ Buy ថ្មីតាមរយៈ CTrade")
        );
    }
}
