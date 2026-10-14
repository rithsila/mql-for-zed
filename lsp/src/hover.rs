use crate::hover_config::{HoverConfig, PrimaryLanguage};
use crate::index::{Kind, Symbol};
use crate::localization::Catalog;
use crate::set_file::InputParam;
use std::path::Path;

fn kind_display_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Function => "Function",
        Kind::Class => "Class",
        Kind::Enum => "Enum",
        Kind::Constant => "Constant",
        Kind::Macro => "Macro",
    }
}

/// Render a single MQL symbol's hover block according to `HoverConfig`.
pub fn render_single_symbol(s: &Symbol, config: &HoverConfig) -> String {
    let primary = config.effective_primary();
    let mut out = String::new();

    // 1. Symbol name and kind
    let raw_kind = kind_display_name(s.kind);
    let kind_label = Catalog::kind_label(primary, raw_kind);
    out.push_str(&format!("**{}** · {}\n\n", s.name, kind_label));

    // Signature code block
    out.push_str("```mql5\n");
    out.push_str(&s.signature);
    out.push_str("\n```");

    // 2 & 3. Explanations according to enabled languages and order
    if config.show_documentation && !config.is_facts_only() {
        let mut sections = Vec::new();

        if config.is_bilingual() {
            // Bilingual mode
            let render_en = || -> String {
                let text = s
                    .localized_doc
                    .en
                    .as_deref()
                    .unwrap_or_else(|| Catalog::unavailable_doc(PrimaryLanguage::English));
                format!(
                    "**{}**\n\n{}",
                    Catalog::language_heading(PrimaryLanguage::English),
                    text
                )
            };

            let render_km = || -> String {
                let text = s
                    .localized_doc
                    .km
                    .as_deref()
                    .unwrap_or_else(|| Catalog::unavailable_doc(PrimaryLanguage::Khmer));
                format!(
                    "**{}**\n\n{}",
                    Catalog::language_heading(PrimaryLanguage::Khmer),
                    text
                )
            };

            match primary {
                PrimaryLanguage::Khmer => {
                    sections.push(render_km());
                    sections.push(render_en());
                }
                PrimaryLanguage::English => {
                    sections.push(render_en());
                    sections.push(render_km());
                }
            }

            // Unclassified documentation available in bilingual mode
            if let Some(unc) = &s.localized_doc.unclassified {
                let heading = Catalog::unclassified_heading(primary);
                sections.push(format!("**{}**\n\n{}", heading, unc));
            }
        } else if config.languages.english {
            // English-only mode
            if let Some(en) = &s.localized_doc.en {
                sections.push(en.clone());
            } else if s.localized_doc.unclassified.is_some() {
                sections.push(Catalog::tagging_notice(PrimaryLanguage::English).to_string());
            } else {
                sections.push(Catalog::unavailable_doc(PrimaryLanguage::English).to_string());
            }
        } else if config.languages.khmer {
            // Khmer-only mode
            if let Some(km) = &s.localized_doc.km {
                sections.push(km.clone());
            } else if s.localized_doc.unclassified.is_some() {
                sections.push(Catalog::tagging_notice(PrimaryLanguage::Khmer).to_string());
            } else {
                sections.push(Catalog::unavailable_doc(PrimaryLanguage::Khmer).to_string());
            }
        }

        if !sections.is_empty() {
            out.push_str("\n\n");
            out.push_str(&sections.join("\n\n"));
        }
    }

    // 4 & 5. Provenance and declaration location
    let mut provenance = Vec::new();

    if config.show_source {
        if let Some((path, line)) = &s.loc {
            let label = Catalog::source_label(primary);
            let display_path = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_else(|| path.to_str().unwrap_or(""));
            provenance.push(format!("{}: {}:{}", label, display_path, line + 1));
        }
    }

    if config.show_documentation {
        let doc_label = Catalog::documentation_label(primary);
        if s.loc.is_none() {
            provenance.push(format!(
                "{}: {}",
                doc_label,
                Catalog::bundled_reference(primary)
            ));
        } else if !s.localized_doc.is_empty() {
            provenance.push(format!(
                "{}: {}",
                doc_label,
                Catalog::source_comment(primary)
            ));
        }
    }

    if !provenance.is_empty() {
        out.push_str("\n\n");
        out.push_str(&provenance.join("\n"));
    }

    out
}

/// Render MQL symbols hover, handling ambiguous candidates gracefully with explicit provenance.
pub fn render_symbol_hover(symbols: &[Symbol], config: &HoverConfig) -> Option<String> {
    if !config.enabled || symbols.is_empty() {
        return None;
    }

    let primary = config.effective_primary();
    let total = symbols.len();
    let limit = 5;
    let candidates: Vec<&Symbol> = symbols.iter().take(limit).collect();

    let mut blocks = Vec::new();
    for (i, sym) in candidates.iter().enumerate() {
        let mut block = String::new();
        if total > 1 {
            block.push_str(&format!(
                "*{}*\n\n",
                Catalog::declaration_index(primary, i + 1, total)
            ));
        }
        block.push_str(&render_single_symbol(sym, config));
        blocks.push(block);
    }

    let mut full = blocks.join("\n\n---\n\n");
    if total > limit {
        full.push_str(&format!(
            "\n\n*{}*",
            Catalog::additional_declarations(primary, total - limit)
        ));
    }

    Some(full)
}

/// Render `.set` file input hover using the shared hover structure and configuration.
pub fn render_set_input_hover(
    param: &InputParam,
    mq5_path: &Path,
    config: &HoverConfig,
) -> Option<String> {
    if !config.enabled {
        return None;
    }

    let primary = config.effective_primary();
    let mut out = String::new();

    let kind_label = Catalog::kind_label(primary, "Input");
    out.push_str(&format!("**{}** · {}\n\n", param.name, kind_label));

    // Code block with declaration
    out.push_str("```mql5\n");
    out.push_str(&format!(
        "input {} {} = {};",
        param.type_name, param.name, param.default_value
    ));
    out.push_str("\n```\n\n");

    // Facts block
    let type_label = Catalog::type_label(primary);
    let default_label = Catalog::default_label(primary);
    out.push_str(&format!(
        "{}: `{}`\n\n{}: `{}`",
        type_label, param.type_name, default_label, param.default_value
    ));

    // Provenance
    let mut provenance = Vec::new();
    if config.show_source {
        let source_label = Catalog::source_label(primary);
        let display_path = mq5_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| mq5_path.to_str().unwrap_or(""));
        provenance.push(format!(
            "{}: {}:{}",
            source_label,
            display_path,
            param.line + 1
        ));
    }
    if config.show_documentation {
        let doc_label = Catalog::documentation_label(primary);
        provenance.push(format!(
            "{}: {}",
            doc_label,
            Catalog::set_parameter_provenance(primary)
        ));
    }

    if !provenance.is_empty() {
        out.push_str("\n\n");
        out.push_str(&provenance.join("\n"));
    }

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hover_config::LanguageConfig;
    use crate::localization::LocalizedDoc;
    use std::path::PathBuf;

    fn sample_macro() -> Symbol {
        Symbol {
            name: "ENG_REC".to_string(),
            kind: Kind::Macro,
            signature: "#define ENG_REC 1".to_string(),
            doc: "Identifier for the recovery engine.".to_string(),
            localized_doc: LocalizedDoc {
                en: Some("Identifier for the recovery engine.".to_string()),
                km: Some("លេខសម្គាល់សម្រាប់ម៉ាស៊ីនស្តារការខាតបង់។".to_string()),
                unclassified: None,
                raw: "@en Identifier for the recovery engine.\n@km លេខសម្គាល់សម្រាប់ម៉ាស៊ីនស្តារការខាតបង់។"
                    .to_string(),
            },
            loc: Some((PathBuf::from("/test/TradeEA.mqh"), 9)),
        }
    }

    #[test]
    fn test_mode_bilingual_english_first() {
        let config = HoverConfig {
            enabled: true,
            languages: LanguageConfig {
                english: true,
                khmer: true,
            },
            primary_language: PrimaryLanguage::English,
            show_source: true,
            show_documentation: true,
        };
        let sym = sample_macro();
        let rendered = render_single_symbol(&sym, &config);

        assert!(rendered.contains("**ENG_REC** · Macro"));
        assert!(rendered.contains("#define ENG_REC 1"));
        assert!(rendered.contains("**English**\n\nIdentifier for the recovery engine."));
        assert!(rendered.contains("**ខ្មែរ**\n\nលេខសម្គាល់សម្រាប់ម៉ាស៊ីនស្តារការខាតបង់។"));
        assert!(rendered.contains("Source: TradeEA.mqh:10"));
        assert!(rendered.contains("Documentation: source comment"));

        // Verify English comes before Khmer in English primary
        let pos_en = rendered.find("**English**").unwrap();
        let pos_km = rendered.find("**ខ្មែរ**").unwrap();
        assert!(pos_en < pos_km);
    }

    #[test]
    fn test_mode_bilingual_khmer_first() {
        let config = HoverConfig {
            enabled: true,
            languages: LanguageConfig {
                english: true,
                khmer: true,
            },
            primary_language: PrimaryLanguage::Khmer,
            show_source: true,
            show_documentation: true,
        };
        let sym = sample_macro();
        let rendered = render_single_symbol(&sym, &config);

        assert!(rendered.contains("**ENG_REC** · ម៉ាក្រូ (Macro)"));
        assert!(rendered.contains("**English**\n\nIdentifier for the recovery engine."));
        assert!(rendered.contains("**ខ្មែរ**\n\nលេខសម្គាល់សម្រាប់ម៉ាស៊ីនស្តារការខាតបង់។"));
        assert!(rendered.contains("ប្រភព: TradeEA.mqh:10"));
        assert!(rendered.contains("ឯកសារយោង: ការអធិប្បាយក្នុងកូដប្រភព"));

        // Verify Khmer comes before English in Khmer primary
        let pos_km = rendered.find("**ខ្មែរ**").unwrap();
        let pos_en = rendered.find("**English**").unwrap();
        assert!(pos_km < pos_en);
    }

    #[test]
    fn test_mode_english_only() {
        let config = HoverConfig {
            enabled: true,
            languages: LanguageConfig {
                english: true,
                khmer: false,
            },
            primary_language: PrimaryLanguage::English,
            show_source: true,
            show_documentation: true,
        };
        let sym = sample_macro();
        let rendered = render_single_symbol(&sym, &config);

        assert!(rendered.contains("Identifier for the recovery engine."));
        assert!(!rendered.contains("លេខសម្គាល់សម្រាប់ម៉ាស៊ីនស្តារការខាតបង់។"));
        assert!(!rendered.contains("**ខ្មែរ**"));
    }

    #[test]
    fn test_mode_khmer_only() {
        let config = HoverConfig {
            enabled: true,
            languages: LanguageConfig {
                english: false,
                khmer: true,
            },
            primary_language: PrimaryLanguage::Khmer,
            show_source: true,
            show_documentation: true,
        };
        let sym = sample_macro();
        let rendered = render_single_symbol(&sym, &config);

        assert!(rendered.contains("លេខសម្គាល់សម្រាប់ម៉ាស៊ីនស្តារការខាតបង់។"));
        assert!(!rendered.contains("Identifier for the recovery engine."));
        assert!(!rendered.contains("**English**"));
    }

    #[test]
    fn test_mode_facts_only() {
        let config = HoverConfig {
            enabled: true,
            languages: LanguageConfig {
                english: false,
                khmer: false,
            },
            primary_language: PrimaryLanguage::English,
            show_source: true,
            show_documentation: true,
        };
        let sym = sample_macro();
        let rendered = render_single_symbol(&sym, &config);

        assert!(rendered.contains("**ENG_REC** · Macro"));
        assert!(rendered.contains("#define ENG_REC 1"));
        assert!(!rendered.contains("Identifier for the recovery engine."));
        assert!(!rendered.contains("លេខសម្គាល់សម្រាប់ម៉ាស៊ីនស្តារការខាតបង់។"));
        assert!(rendered.contains("Source: TradeEA.mqh:10"));
    }

    #[test]
    fn test_mode_hover_disabled() {
        let config = HoverConfig {
            enabled: false,
            ..Default::default()
        };
        let sym = sample_macro();
        assert!(render_symbol_hover(&[sym], &config).is_none());
    }

    #[test]
    fn test_missing_translation_notice() {
        let sym_without_km = Symbol {
            name: "Foo".to_string(),
            kind: Kind::Function,
            signature: "void Foo()".to_string(),
            doc: "Does foo.".to_string(),
            localized_doc: LocalizedDoc {
                en: Some("Does foo.".to_string()),
                km: None,
                unclassified: None,
                raw: "Does foo.".to_string(),
            },
            loc: None,
        };

        // In Khmer only mode: shows missing Khmer translation notice
        let km_config = HoverConfig {
            enabled: true,
            languages: LanguageConfig {
                english: false,
                khmer: true,
            },
            primary_language: PrimaryLanguage::Khmer,
            show_source: true,
            show_documentation: true,
        };
        let rendered = render_single_symbol(&sym_without_km, &km_config);
        assert!(rendered.contains("មិនទាន់មានការបកប្រែជាភាសាខ្មែរ"));
        assert!(!rendered.contains("Does foo.")); // English must NOT leak!
    }

    #[test]
    fn test_set_input_hover() {
        let param = InputParam {
            name: "InpMagic".to_string(),
            type_name: "ulong".to_string(),
            default_value: "123456".to_string(),
            line: 5,
            character: 6,
        };
        let config = HoverConfig::default();
        let rendered =
            render_set_input_hover(&param, Path::new("/test/MyEA.mq5"), &config).unwrap();

        assert!(rendered.contains("**InpMagic** · Input"));
        assert!(rendered.contains("input ulong InpMagic = 123456;"));
        assert!(rendered.contains("Type: `ulong`"));
        assert!(rendered.contains("Default: `123456`"));
        assert!(rendered.contains("Source: MyEA.mq5:6"));
        assert!(rendered.contains("Documentation: .set parameter definition"));
    }
}
