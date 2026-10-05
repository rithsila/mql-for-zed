use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct InputParam {
    pub name: String,
    pub type_name: String,
    pub default_value: String,
    pub line: u32,
    pub character: u32,
}

pub fn parse_mq5_inputs(text: &str) -> Vec<InputParam> {
    let re = Regex::new(r"(?m)^\s*s?input\s+(?P<type>[A-Za-z_][\w ]*?)\s+(?P<name>[A-Za-z_]\w*)\s*=\s*(?P<rest>.*)$").unwrap();
    let mut inputs = Vec::new();

    // To calculate line numbers, we can just find line starts
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();

    for cap in re.captures_iter(text) {
        if cap["type"].trim() == "group" {
            continue;
        }
        let rest = cap["rest"].to_string();
        let val = rest.split(';').next().unwrap_or("").trim().to_string();

        let match_start = cap.get(0).unwrap().start();
        let name_start = cap.name("name").unwrap().start();

        // Find line number
        let line_idx = match line_starts.binary_search(&match_start) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        let line = line_idx as u32;
        let character = (name_start - line_starts[line_idx]) as u32;

        inputs.push(InputParam {
            name: cap["name"].to_string(),
            type_name: cap["type"].trim().to_string(),
            default_value: val,
            line,
            character,
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
                let full_value_str = line[eq_idx + 1..].trim();
                let parts: Vec<&str> = full_value_str.split("||").map(|s| s.trim()).collect();
                let value_part = parts[0];

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
                                let start_pos = line.find(value_part).unwrap_or(eq_idx + 1);
                                diagnostics.push(Diagnostic {
                                    range: Range::new(
                                        Position::new(i as u32, start_pos as u32),
                                        Position::new(
                                            i as u32,
                                            (start_pos + value_part.len()) as u32,
                                        ),
                                    ),
                                    severity: Some(DiagnosticSeverity::WARNING),
                                    message: format!(
                                        "Type mismatch: '{}' expects a numeric value.",
                                        key
                                    ),
                                    ..Default::default()
                                });
                            }

                            // Optimization fields validation
                            if parts.len() == 5 {
                                let start_val = parts[1].parse::<f64>();
                                let step_val = parts[2].parse::<f64>();
                                let stop_val = parts[3].parse::<f64>();
                                let enabled = parts[4].to_uppercase() == "Y";

                                if enabled {
                                    if let (Ok(start_num), Ok(step_num), Ok(stop_num)) =
                                        (&start_val, &step_val, &stop_val)
                                    {
                                        if *step_num == 0.0 {
                                            let start_pos =
                                                line.find(parts[2]).unwrap_or(eq_idx + 1);
                                            diagnostics.push(Diagnostic {
                                                range: Range::new(
                                                    Position::new(i as u32, start_pos as u32),
                                                    Position::new(
                                                        i as u32,
                                                        (start_pos + parts[2].len()) as u32,
                                                    ),
                                                ),
                                                severity: Some(DiagnosticSeverity::WARNING),
                                                message: format!(
                                                    "Optimization step for '{}' cannot be 0.",
                                                    key
                                                ),
                                                ..Default::default()
                                            });
                                        } else if *step_num > 0.0 && start_num > stop_num {
                                            let start_pos =
                                                line.find(parts[1]).unwrap_or(eq_idx + 1);
                                            diagnostics.push(Diagnostic {
                                                range: Range::new(
                                                    Position::new(i as u32, start_pos as u32),
                                                    Position::new(i as u32, line.len() as u32),
                                                ),
                                                severity: Some(DiagnosticSeverity::WARNING),
                                                message: format!("Invalid optimization range for '{}': start > stop with positive step.", key),
                                                ..Default::default()
                                            });
                                        } else if *step_num < 0.0 && start_num < stop_num {
                                            let start_pos =
                                                line.find(parts[1]).unwrap_or(eq_idx + 1);
                                            diagnostics.push(Diagnostic {
                                                range: Range::new(
                                                    Position::new(i as u32, start_pos as u32),
                                                    Position::new(i as u32, line.len() as u32),
                                                ),
                                                severity: Some(DiagnosticSeverity::WARNING),
                                                message: format!("Invalid optimization range for '{}': start < stop with negative step.", key),
                                                ..Default::default()
                                            });
                                        }
                                    } else {
                                        let start_pos =
                                            line.find(full_value_str).unwrap_or(eq_idx + 1);
                                        diagnostics.push(Diagnostic {
                                            range: Range::new(
                                                Position::new(i as u32, start_pos as u32),
                                                Position::new(i as u32, (start_pos + full_value_str.len()) as u32),
                                            ),
                                            severity: Some(DiagnosticSeverity::WARNING),
                                            message: format!("Optimization fields for numeric parameter '{}' must be numbers.", key),
                                            ..Default::default()
                                        });
                                    }
                                }
                            }
                        } else if param.type_name == "bool" {
                            let v_lower = value_part.to_lowercase();
                            if v_lower != "true"
                                && v_lower != "false"
                                && v_lower != "0"
                                && v_lower != "1"
                            {
                                let start_pos = line.find(value_part).unwrap_or(eq_idx + 1);
                                diagnostics.push(Diagnostic {
                                    range: Range::new(
                                        Position::new(i as u32, start_pos as u32),
                                        Position::new(i as u32, (start_pos + value_part.len()) as u32),
                                    ),
                                    severity: Some(DiagnosticSeverity::WARNING),
                                    message: format!("Type mismatch: '{}' expects a boolean value (true/false/0/1).", key),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                } else {
                    let start_pos = line.find(key).unwrap_or(0);
                    diagnostics.push(Diagnostic {
                        range: Range::new(
                            Position::new(i as u32, start_pos as u32),
                            Position::new(i as u32, (start_pos + key.len()) as u32),
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
