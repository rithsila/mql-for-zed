use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct InputParam {
    pub name: String,
    pub type_name: String,
    pub default_value: String,
}

pub fn parse_mq5_inputs(text: &str) -> Vec<InputParam> {
    let re = Regex::new(r"(?m)^\s*s?input\s+(?P<type>[A-Za-z_][\w ]*?)\s+(?P<name>[A-Za-z_]\w*)\s*=\s*(?P<rest>.*)$").unwrap();
    let mut inputs = Vec::new();
    for cap in re.captures_iter(text) {
        if cap["type"].trim() == "group" {
            continue;
        }
        let rest = cap["rest"].to_string();
        let val = rest.split(';').next().unwrap_or("").trim().to_string();
        inputs.push(InputParam {
            name: cap["name"].to_string(),
            type_name: cap["type"].trim().to_string(),
            default_value: val,
        });
    }
    inputs
}

pub fn get_mq5_inputs_for_set_file(set_path: &Path) -> Option<Vec<InputParam>> {
    let mq5_path = set_path.with_extension("mq5");
    if mq5_path.exists() {
        if let Ok(text) = fs::read_to_string(mq5_path) {
            return Some(parse_mq5_inputs(&text));
        }
    }
    None
}

pub fn check(text: &str, set_path: &Path) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if let Some(inputs) = get_mq5_inputs_for_set_file(set_path) {
        let known_keys: HashMap<String, InputParam> = inputs
            .into_iter()
            .map(|inp| (inp.name.clone(), inp))
            .collect();

        let mut seen_keys = HashMap::new();

        for (i, line) in text.lines().enumerate() {
            let line_trimmed = line.trim();
            if line_trimmed.is_empty() || line_trimmed.starts_with(';') {
                continue;
            }
            if let Some(eq_idx) = line.find('=') {
                let key = line[..eq_idx].trim();
                let mut value_part = line[eq_idx + 1..].trim();
                if let Some(pipe_idx) = value_part.find("||") {
                    value_part = value_part[..pipe_idx].trim();
                }

                let start = line.find(key).unwrap_or(0);
                if let Some(prev_line) = seen_keys.insert(key.to_string(), i) {
                    diagnostics.push(Diagnostic {
                        range: Range::new(
                            Position::new(i as u32, start as u32),
                            Position::new(i as u32, (start + key.len()) as u32),
                        ),
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: format!(
                            "Duplicate parameter '{}'. Was previously defined on line {}.",
                            key,
                            prev_line + 1
                        ),
                        ..Default::default()
                    });
                }

                if let Some(param) = known_keys.get(key) {
                    if !value_part.is_empty() {
                        let is_numeric = param.type_name.contains("int")
                            || param.type_name.contains("double")
                            || param.type_name.contains("float")
                            || param.type_name.contains("long")
                            || param.type_name.contains("short")
                            || param.type_name.contains("char");
                        if is_numeric {
                            if value_part.parse::<f64>().is_err() {
                                let start = line.find(value_part).unwrap_or(eq_idx + 1);
                                diagnostics.push(Diagnostic {
                                    range: Range::new(
                                        Position::new(i as u32, start as u32),
                                        Position::new(i as u32, (start + value_part.len()) as u32),
                                    ),
                                    severity: Some(DiagnosticSeverity::WARNING),
                                    message: format!(
                                        "Type mismatch: '{}' expects a numeric value.",
                                        key
                                    ),
                                    ..Default::default()
                                });
                            }
                        } else if param.type_name == "bool" {
                            let v_lower = value_part.to_lowercase();
                            if v_lower != "true"
                                && v_lower != "false"
                                && v_lower != "0"
                                && v_lower != "1"
                            {
                                let start = line.find(value_part).unwrap_or(eq_idx + 1);
                                diagnostics.push(Diagnostic {
                                    range: Range::new(
                                        Position::new(i as u32, start as u32),
                                        Position::new(i as u32, (start + value_part.len()) as u32),
                                    ),
                                    severity: Some(DiagnosticSeverity::WARNING),
                                    message: format!("Type mismatch: '{}' expects a boolean value (true/false/0/1).", key),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                } else {
                    let start = line.find(key).unwrap_or(0);
                    diagnostics.push(Diagnostic {
                        range: Range::new(
                            Position::new(i as u32, start as u32),
                            Position::new(i as u32, (start + key.len()) as u32),
                        ),
                        severity: Some(DiagnosticSeverity::WARNING),
                        message: format!("Unknown parameter '{}'. This will be ignored by the tester unless it exists in the EA.", key),
                        ..Default::default()
                    });
                }
            }
        }
    }
    diagnostics
}
