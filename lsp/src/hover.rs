use crate::hover_config::{HoverConfig, PrimaryLanguage};
use crate::index::{Kind, Symbol};
use crate::localization::Catalog;
use crate::set_file::InputParam;
use lsp_types::{Position, Range};
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
    if config.show_documentation && !config.is_facts_only() && !s.localized_doc.is_empty() {
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

        // Vibe Check (trading context / plain-language summary) if available
        let vibe_text = s
            .localized_doc
            .vibe
            .as_deref()
            .map(String::from)
            .or_else(|| {
                if s.loc.is_some() {
                    crate::explainer::heuristic_vibe(&s.name, s.kind)
                } else {
                    None
                }
            });

        if let Some(vibe) = vibe_text {
            let label = Catalog::vibe_label(primary);
            sections.push(format!("💡 **{}**\n\n{}", label, vibe));
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
            let source_type = if s.localized_doc.vibe.is_some() {
                Catalog::ai_knowledge_label(primary)
            } else {
                Catalog::source_comment(primary)
            };
            provenance.push(format!("{}: {}", doc_label, source_type));
        }
    }

    if !provenance.is_empty() {
        out.push_str("\n\n");
        out.push_str(&provenance.join("\n"));
    }

    out
}

/// Deduplicate identical or redundant symbol declarations across indexing passes or includes.
pub fn deduplicate_symbols<'a>(symbols: &'a [Symbol]) -> Vec<&'a Symbol> {
    let mut deduped: Vec<&'a Symbol> = Vec::new();
    for sym in symbols {
        let is_dup = deduped.iter().any(|existing| {
            (existing.signature == sym.signature && existing.loc == sym.loc)
                || (existing.signature == sym.signature
                    && existing.name == sym.name
                    && existing.kind == sym.kind
                    && existing.localized_doc == sym.localized_doc)
        });
        if !is_dup {
            deduped.push(sym);
        }
    }
    deduped
}

/// Render MQL symbols hover, handling ambiguous candidates gracefully with explicit provenance.
pub fn render_symbol_hover(symbols: &[Symbol], config: &HoverConfig) -> Option<String> {
    if !config.enabled || symbols.is_empty() {
        return None;
    }

    let deduped = deduplicate_symbols(symbols);
    if deduped.is_empty() {
        return None;
    }

    let primary = config.effective_primary();
    let total = deduped.len();
    let limit = 5;
    let candidates: Vec<&Symbol> = deduped.into_iter().take(limit).collect();

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

fn utf16_col(line: &str, byte_offset: usize) -> u32 {
    let safe_offset = byte_offset.min(line.len());
    line[..safe_offset]
        .chars()
        .map(|c| c.len_utf16() as u32)
        .sum()
}

fn find_keyword_pos(line: &str, kw: &str) -> Option<usize> {
    let mut search_idx = 0;
    while let Some(idx) = line[search_idx..].find(kw) {
        let actual_idx = search_idx + idx;
        let before_ok = if actual_idx == 0 {
            true
        } else {
            let prev = line[..actual_idx].chars().last().unwrap();
            !prev.is_alphanumeric() && prev != '_'
        };
        let after_idx = actual_idx + kw.len();
        let after_ok = if after_idx >= line.len() {
            true
        } else {
            let next = line[after_idx..].chars().next().unwrap();
            !next.is_alphanumeric() && next != '_'
        };
        if before_ok && after_ok {
            return Some(actual_idx);
        }
        search_idx = actual_idx + 1;
    }
    None
}

fn extract_paren_enclosed(
    lines: &[&str],
    start_line: usize,
    kw_byte_idx: usize,
) -> Option<(String, Range)> {
    let first_line = lines.get(start_line)?;
    let start_col = utf16_col(first_line, kw_byte_idx);

    let mut depth = 0;
    let mut found_open = false;
    let mut end_line = start_line;
    let mut end_byte_idx = 0;
    let mut snippet_lines = Vec::new();

    for (rel_idx, l) in lines[start_line..].iter().enumerate() {
        let l_idx = start_line + rel_idx;
        let scan_start = if l_idx == start_line { kw_byte_idx } else { 0 };
        let mut in_str = false;
        let mut str_char = '"';
        let mut chars = l[scan_start..].char_indices().peekable();

        while let Some((rel_b, ch)) = chars.next() {
            let abs_b = scan_start + rel_b;

            if in_str {
                if ch == '\\' {
                    let _ = chars.next();
                } else if ch == str_char {
                    in_str = false;
                }
                continue;
            } else if ch == '"' || ch == '\'' {
                in_str = true;
                str_char = ch;
                continue;
            }

            if ch == '/' && chars.peek().map_or(false, |(_, c2)| *c2 == '/') {
                break;
            }

            if ch == '(' {
                depth += 1;
                found_open = true;
            } else if ch == ')' && found_open {
                depth -= 1;
                if depth == 0 {
                    end_line = l_idx;
                    end_byte_idx = abs_b + ch.len_utf8();
                    break;
                }
            }
        }

        if l_idx == start_line {
            if end_line == start_line && found_open && depth == 0 {
                snippet_lines.push(l[kw_byte_idx..end_byte_idx].trim().to_string());
                break;
            } else {
                snippet_lines.push(l[kw_byte_idx..].trim_end().to_string());
            }
        } else if end_line == l_idx && found_open && depth == 0 {
            snippet_lines.push(l[..end_byte_idx].trim_start().to_string());
            break;
        } else {
            snippet_lines.push(l.trim().to_string());
        }

        if l_idx - start_line >= 10 {
            break;
        }
    }

    if found_open && depth == 0 {
        let last_line = lines.get(end_line)?;
        let end_col = utf16_col(last_line, end_byte_idx);
        let range = Range::new(
            Position::new(start_line as u32, start_col),
            Position::new(end_line as u32, end_col),
        );
        let snippet = snippet_lines.join(" ");
        Some((snippet, range))
    } else {
        None
    }
}

/// Renders context-aware hover explanation for statements, conditions, loops, and control flow.
pub fn render_statement_hover(
    doc_text: &str,
    pos: Position,
    config: &HoverConfig,
) -> Option<(String, Range)> {
    if !config.enabled {
        return None;
    }
    let lines: Vec<&str> = doc_text.lines().collect();
    let line_idx = pos.line as usize;
    if line_idx >= lines.len() {
        return None;
    }
    let line = lines[line_idx];
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") {
        return None;
    }

    // 1. Condition check: 'if'
    if let Some(if_idx) = find_keyword_pos(line, "if") {
        if let Some((snippet, range)) = extract_paren_enclosed(&lines, line_idx, if_idx) {
            let hover_text = crate::explainer::explain_statement_hover(&snippet, config)?;
            return Some((hover_text, range));
        }
    }

    // Check if current line is part of a multiline 'if' condition by scanning upward a few lines
    for back in 1..=5 {
        if line_idx >= back {
            let prev_idx = line_idx - back;
            let prev_line = lines[prev_idx];
            if let Some(if_idx) = find_keyword_pos(prev_line, "if") {
                if let Some((snippet, range)) = extract_paren_enclosed(&lines, prev_idx, if_idx) {
                    if pos.line <= range.end.line {
                        let hover_text =
                            crate::explainer::explain_statement_hover(&snippet, config)?;
                        return Some((hover_text, range));
                    }
                }
            }
        }
    }

    // 2. Loop check: 'for' or 'while'
    if let Some(for_idx) = find_keyword_pos(line, "for") {
        if let Some((snippet, range)) = extract_paren_enclosed(&lines, line_idx, for_idx) {
            let hover_text = crate::explainer::explain_statement_hover(&snippet, config)?;
            return Some((hover_text, range));
        }
    }
    if let Some(while_idx) = find_keyword_pos(line, "while") {
        if let Some((snippet, range)) = extract_paren_enclosed(&lines, line_idx, while_idx) {
            let hover_text = crate::explainer::explain_statement_hover(&snippet, config)?;
            return Some((hover_text, range));
        }
    }

    // 3. Return statement
    if let Some(ret_idx) = find_keyword_pos(line, "return") {
        let semi_idx = line[ret_idx..]
            .find(';')
            .map(|i| ret_idx + i + 1)
            .unwrap_or(line.len());
        let snippet = line[ret_idx..semi_idx].trim().to_string();
        let start_col = utf16_col(line, ret_idx);
        let end_col = utf16_col(line, semi_idx.min(line.len()));
        let range = Range::new(
            Position::new(pos.line, start_col),
            Position::new(pos.line, end_col),
        );
        let hover_text = crate::explainer::explain_statement_hover(&snippet, config)?;
        return Some((hover_text, range));
    }

    // 4. Struct or Class declaration
    if let Some(k_idx) =
        find_keyword_pos(line, "struct").or_else(|| find_keyword_pos(line, "class"))
    {
        let brace_idx = line[k_idx..]
            .find('{')
            .map(|i| k_idx + i)
            .unwrap_or(line.len());
        let snippet = line[k_idx..brace_idx].trim().to_string();
        let start_col = utf16_col(line, k_idx);
        let end_col = utf16_col(line, brace_idx.min(line.len()));
        let range = Range::new(
            Position::new(pos.line, start_col),
            Position::new(pos.line, end_col),
        );
        let hover_text = crate::explainer::explain_statement_hover(&snippet, config)?;
        return Some((hover_text, range));
    }

    // 5. Input parameter declaration
    if let Some(k_idx) =
        find_keyword_pos(line, "input").or_else(|| find_keyword_pos(line, "sinput"))
    {
        let semi_idx = line[k_idx..]
            .find(';')
            .map(|i| k_idx + i + 1)
            .unwrap_or(line.len());
        let snippet = line[k_idx..semi_idx].trim().to_string();
        let start_col = utf16_col(line, k_idx);
        let end_col = utf16_col(line, semi_idx.min(line.len()));
        let range = Range::new(
            Position::new(pos.line, start_col),
            Position::new(pos.line, end_col),
        );
        let hover_text = crate::explainer::explain_statement_hover(&snippet, config)?;
        return Some((hover_text, range));
    }

    // 6. General trading expression or keyword
    if line.contains("&&")
        || line.contains("||")
        || line.contains("!=")
        || line.contains("==")
        || line.contains("PositionsTotal")
        || line.contains("OrdersTotal")
        || line.contains("OrderSend")
        || line.contains("NormalizeDouble")
        || line.contains("CopyBuffer")
        || line.contains("CopyRates")
        || line.contains("SymbolInfoDouble")
        || find_keyword_pos(line, "switch").is_some()
        || find_keyword_pos(line, "case").is_some()
    {
        let snippet = trimmed.to_string();
        let leading_ws = line.len() - line.trim_start().len();
        let start_col = utf16_col(line, leading_ws);
        let end_col = utf16_col(line, line.trim_end().len());
        let range = Range::new(
            Position::new(pos.line, start_col),
            Position::new(pos.line, end_col),
        );
        let hover_text = crate::explainer::explain_statement_hover(&snippet, config)?;
        return Some((hover_text, range));
    }

    None
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
                vibe: None,
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
                vibe: None,
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

    #[test]
    fn test_user_symbol_without_docs_has_no_negative_boilerplate() {
        let custom_struct = Symbol {
            name: "SymbolContext".to_string(),
            kind: Kind::Class,
            signature: "class SymbolContext".to_string(),
            doc: String::new(),
            localized_doc: LocalizedDoc::default(),
            loc: Some((PathBuf::from("/test/waka_waka.mq5"), 141)),
        };

        let config = HoverConfig::default(); // Bilingual by default
        let rendered = render_single_symbol(&custom_struct, &config);

        assert!(rendered.contains("**SymbolContext** · Class"));
        assert!(rendered.contains("class SymbolContext"));
        assert!(rendered.contains("Source: waka_waka.mq5:142"));
        // Clean output: NO negative boilerplate
        assert!(!rendered.contains("Documentation is not available"));
        assert!(!rendered.contains("មិនទាន់មានការបកប្រែ"));
        assert!(!rendered.contains("Documentation:"));
    }

    #[test]
    fn test_deduplicate_symbols_collapses_duplicates() {
        let sym1 = Symbol {
            name: "SymbolContext".to_string(),
            kind: Kind::Class,
            signature: "class SymbolContext".to_string(),
            doc: String::new(),
            localized_doc: LocalizedDoc::default(),
            loc: Some((PathBuf::from("/test/waka_waka.mq5"), 141)),
        };
        let sym2 = sym1.clone();
        let sym3 = sym1.clone();

        let config = HoverConfig::default();
        let rendered = render_symbol_hover(&[sym1, sym2, sym3], &config).unwrap();

        // Must collapse to a single declaration with NO "Declaration 1 of 3"
        assert!(!rendered.contains("Declaration 1 of"));
        assert!(!rendered.contains("Declaration 2 of"));
        assert!(!rendered.contains("---"));
        assert!(rendered.contains("**SymbolContext** · Class"));
    }

    #[test]
    fn test_vibe_check_rendering() {
        let sym = Symbol {
            name: "SymbolContext".to_string(),
            kind: Kind::Class,
            signature: "class SymbolContext".to_string(),
            doc: "Holds trading settings.".to_string(),
            localized_doc: LocalizedDoc {
                en: Some("Holds trading settings for a currency pair.".to_string()),
                km: Some("ផ្ទុកការកំណត់ការជួញដូរសម្រាប់គូរូបិយប័ណ្ណមួយ។".to_string()),
                unclassified: None,
                vibe: Some("Everything the robot needs to know about this pair.".to_string()),
                raw: String::new(),
            },
            loc: Some((PathBuf::from("/test/waka_waka.mq5"), 141)),
        };

        let config = HoverConfig::default();
        let rendered = render_single_symbol(&sym, &config);

        assert!(rendered.contains("💡 **Vibe Check (Trading Context)**"));
        assert!(rendered.contains("Everything the robot needs to know about this pair."));
        assert!(rendered.contains("Documentation: AI Workspace Knowledge (.mql/knowledge.json)"));
    }

    #[test]
    fn test_statement_hover_condition_bilingual() {
        let code = "   if(AllowHedging && mm != ACCOUNT_MARGIN_MODE_RETAIL_HEDGING)\n   {\n      Print(\"Hedging disabled\");\n   }";
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

        let pos = Position::new(0, 5); // on 'if'
        let (hover_text, range) = render_statement_hover(code, pos, &config).unwrap();

        assert!(hover_text.contains("Condition Gate"));
        assert!(hover_text.contains("if(AllowHedging && mm != ACCOUNT_MARGIN_MODE_RETAIL_HEDGING)"));
        assert!(hover_text.contains("AllowHedging"));
        assert!(hover_text.contains("ACCOUNT_MARGIN_MODE_RETAIL_HEDGING"));
        assert!(hover_text.contains("**English**"));
        assert!(hover_text.contains("**ខ្មែរ**"));
        assert!(hover_text.contains("Trader Gotcha"));
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 3); // 'if' starts at col 3
    }

    #[test]
    fn test_statement_hover_loop_reverse() {
        let code = "for(int i = PositionsTotal() - 1; i >= 0; i--)\n{\n}";
        let config = HoverConfig::default();
        let pos = Position::new(0, 10);
        let (hover_text, range) = render_statement_hover(code, pos, &config).unwrap();

        assert!(hover_text.contains("Loop Iteration"));
        assert!(hover_text.contains("PositionsTotal"));
        assert!(hover_text.contains("reverse loops"));
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
    }

    #[test]
    fn test_statement_hover_return() {
        let code = "   return INIT_FAILED;";
        let config = HoverConfig::default();
        let pos = Position::new(0, 5);
        let (hover_text, range) = render_statement_hover(code, pos, &config).unwrap();

        assert!(hover_text.contains("Return Statement"));
        assert!(hover_text.contains("INIT_FAILED"));
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 3);
    }

    #[test]
    fn test_statement_hover_disabled() {
        let code = "if(AllowHedging) {}";
        let config = HoverConfig {
            enabled: false,
            ..Default::default()
        };
        let pos = Position::new(0, 2);
        assert!(render_statement_hover(code, pos, &config).is_none());
    }

    #[test]
    fn test_statement_hover_facts_only() {
        let code = "if(AllowHedging) {}";
        let config = HoverConfig {
            enabled: true,
            languages: LanguageConfig {
                english: false,
                khmer: false,
            },
            ..Default::default()
        };
        let pos = Position::new(0, 2);
        let (hover_text, _) = render_statement_hover(code, pos, &config).unwrap();

        assert!(hover_text.contains("Condition Gate"));
        assert!(hover_text.contains("```mql5\nif(AllowHedging)\n```"));
        // Facts only: no prose explanations
        assert!(!hover_text.contains("**English**"));
        assert!(!hover_text.contains("**ខ្មែរ**"));
    }
}
