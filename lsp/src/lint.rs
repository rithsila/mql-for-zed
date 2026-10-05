use lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range, Url};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;

pub const SOURCE: &str = "zed-mql-lint";

pub const RULE_IGNORED_TRADE: &str = "ignored-trade-result";
pub const RULE_UNINSPECTED_TRADE: &str = "uninspected-trade-result";
pub const RULE_UNCHECKED_COPY_BUFFER: &str = "unchecked-copy-buffer";
pub const RULE_UNCHECKED_INDICATOR_HANDLE: &str = "unchecked-indicator-handle";
pub const RULE_INDICATOR_IN_ONTICK: &str = "indicator-in-ontick";

#[derive(Clone, Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuleConfig {
    pub enabled: Option<bool>,
    pub severity: Option<String>,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub rules: HashMap<String, RuleConfig>,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            rules: HashMap::new(),
        }
    }
}

pub fn check(text: &str, _uri: &Url, config: &Config) -> Vec<Diagnostic> {
    if !config.enabled {
        return vec![];
    }

    let mut diagnostics = Vec::new();

    let re_suppress = Regex::new(r"//\s*zed-mql-lint:\s*disable\s+([a-z0-9_-]+)").unwrap();

    // Rule 1: Ignored trade return value
    // Matches expressions starting with (receiver.)trade_function(...) without assignment or control condition
    let re_ignored_trade = Regex::new(
        r"^\s*(?:[A-Za-z_]\w*(?:\.|->)|[A-Za-z_]\w*::)?(OrderSend|PositionOpen|Buy|Sell|BuyLimit|SellLimit|BuyStop|SellStop|PositionModify|PositionClose)\s*\(",
    )
    .unwrap();

    // Rule 2: Trade API return checked as bool without inspecting result retcode
    let re_trade_bool_check = Regex::new(
        r"if\s*\(\s*(?:!|\s)*(?:[A-Za-z_]\w*(?:\.|->)|[A-Za-z_]\w*::)?(OrderSend|PositionOpen|Buy|Sell|BuyLimit|SellLimit|BuyStop|SellStop|PositionModify|PositionClose)\s*\(",
    )
    .unwrap();

    // Rule 3: CopyBuffer / CopyRates
    let re_direct_copy = Regex::new(r"^\s*(CopyBuffer|CopyRates)\s*\(").unwrap();
    let re_assigned_copy = Regex::new(
        r"(?:int\s+)?([A-Za-z_]\w*)\s*=\s*(CopyBuffer|CopyRates)\s*\([^,]+,[^,]+,[^,]+,[^,]+,\s*([A-Za-z_]\w*)",
    )
    .unwrap();

    // Indicator constructors
    let re_indicator_call = Regex::new(
        r"\b(iMA|iCustom|iRSI|iMACD|iATR|iBands|iBearsPower|iBullsPower|iCCI|iDeMarker|iEnvelopes|iForce|iFractals|iIchimoku|iMomentum|iOsMA|iStochastic|iWPR|iAlligator|iADX|iStdDev)\s*\(",
    )
    .unwrap();

    let re_indicator_assign = Regex::new(
        r"([A-Za-z_]\w*)\s*=\s*(iMA|iCustom|iRSI|iMACD|iATR|iBands|iBearsPower|iBullsPower|iCCI|iDeMarker|iEnvelopes|iForce|iFractals|iIchimoku|iMomentum|iOsMA|iStochastic|iWPR|iAlligator|iADX|iStdDev)\s*\(",
    )
    .unwrap();

    let re_ontick = Regex::new(r"^\s*(?:void|int|double)\s+OnTick\s*\(").unwrap();

    let lines: Vec<&str> = text.lines().collect();

    let get_severity = |rule_id: &str, default: DiagnosticSeverity| -> Option<DiagnosticSeverity> {
        if let Some(r) = config.rules.get(rule_id) {
            if r.enabled == Some(false) {
                return None;
            }
            match r.severity.as_deref() {
                Some("error") => return Some(DiagnosticSeverity::ERROR),
                Some("warning") => return Some(DiagnosticSeverity::WARNING),
                Some("information") => return Some(DiagnosticSeverity::INFORMATION),
                Some("hint") => return Some(DiagnosticSeverity::HINT),
                _ => {}
            }
        }
        Some(default)
    };

    let add_diag = |diagnostics: &mut Vec<Diagnostic>,
                    line: u32,
                    id: &'static str,
                    message: &'static str,
                    severity: DiagnosticSeverity| {
        if let Some(sev) = get_severity(id, severity) {
            let pos = Position::new(line, 0);
            diagnostics.push(Diagnostic {
                range: Range::new(pos, pos),
                severity: Some(sev),
                code: Some(NumberOrString::String(id.to_string())),
                source: Some(SOURCE.into()),
                message: message.into(),
                ..Default::default()
            });
        }
    };

    let mut in_ontick = false;
    let mut depth = 0;

    for (i, line) in lines.iter().enumerate() {
        let line_num = i as u32;

        if re_ontick.is_match(line) {
            in_ontick = true;
            depth = 0;
        }

        depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
        if in_ontick && depth <= 0 && line.contains('}') {
            in_ontick = false;
        }

        // Rule 1: Ignored trade return value
        if re_ignored_trade.is_match(line) {
            add_diag(
                &mut diagnostics,
                line_num,
                RULE_IGNORED_TRADE,
                "Ignored return value from direct OrderSend() or CTrade method. Check execution result and error code.",
                DiagnosticSeverity::WARNING,
            );
        }

        // Rule 2: Trade boolean check without checking retcode
        if re_trade_bool_check.is_match(line) {
            // Check if this line or the following 8 lines inspect retcode/result
            let mut inspected = line.contains("ResultRetcode")
                || line.contains("retcode")
                || line.contains("TRADE_RETCODE")
                || line.contains("ResultDeal")
                || line.contains("ResultOrder");

            if !inspected {
                let lookahead_limit = (i + 10).min(lines.len());
                for next_line in &lines[i + 1..lookahead_limit] {
                    if next_line.contains("ResultRetcode")
                        || next_line.contains("retcode")
                        || next_line.contains("TRADE_RETCODE")
                        || next_line.contains("ResultDeal")
                        || next_line.contains("ResultOrder")
                    {
                        inspected = true;
                        break;
                    }
                }
            }

            if !inspected {
                add_diag(
                    &mut diagnostics,
                    line_num,
                    RULE_UNINSPECTED_TRADE,
                    "API return value treated as execution success without inspecting actual return codes or results (e.g. ResultRetcode() == TRADE_RETCODE_DONE).",
                    DiagnosticSeverity::WARNING,
                );
            }
        }

        // Rule 3: Unchecked CopyBuffer/CopyRates
        if re_direct_copy.is_match(line) {
            add_diag(
                &mut diagnostics,
                line_num,
                RULE_UNCHECKED_COPY_BUFFER,
                "Ignored or unchecked element count from CopyBuffer/CopyRates. Verify returned count > 0 before using buffer.",
                DiagnosticSeverity::WARNING,
            );
        } else if let Some(caps) = re_assigned_copy.captures(line) {
            let count_var = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let buf_var = caps.get(3).map(|m| m.as_str()).unwrap_or("");

            // Check if buffer is consumed in next lines before checking count_var
            let lookahead_limit = (i + 8).min(lines.len());
            let buf_access = format!("{}[", buf_var);
            let mut count_checked = false;
            let mut buf_used_unchecked = false;

            for next_line in &lines[i + 1..lookahead_limit] {
                if next_line.contains(count_var)
                    && (next_line.contains("<=")
                        || next_line.contains("<")
                        || next_line.contains(">")
                        || next_line.contains("=="))
                {
                    count_checked = true;
                    break;
                }
                if next_line.contains(&buf_access) {
                    buf_used_unchecked = true;
                    break;
                }
            }

            if buf_used_unchecked && !count_checked {
                add_diag(
                    &mut diagnostics,
                    line_num,
                    RULE_UNCHECKED_COPY_BUFFER,
                    "Buffer accessed without verifying returned element count from CopyBuffer/CopyRates is greater than 0.",
                    DiagnosticSeverity::WARNING,
                );
            }
        }

        // Rule 4: Indicator handle initialization without INVALID_HANDLE check
        if let Some(caps) = re_indicator_assign.captures(line) {
            let handle_var = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let mut handle_checked = line.contains("INVALID_HANDLE");

            if !handle_checked {
                let lookahead_limit = (i + 6).min(lines.len());
                for next_line in &lines[i + 1..lookahead_limit] {
                    if next_line.contains(handle_var)
                        && (next_line.contains("INVALID_HANDLE")
                            || next_line.contains("!= 0")
                            || next_line.contains("< 0")
                            || next_line.contains("<= 0"))
                    {
                        handle_checked = true;
                        break;
                    }
                }
            }

            if !handle_checked {
                add_diag(
                    &mut diagnostics,
                    line_num,
                    RULE_UNCHECKED_INDICATOR_HANDLE,
                    "Indicator handle initialized without checking for INVALID_HANDLE.",
                    DiagnosticSeverity::WARNING,
                );
            }
        }

        // Rule 5: Repeated indicator construction in OnTick
        if in_ontick && re_indicator_call.is_match(line) {
            add_diag(
                &mut diagnostics,
                line_num,
                RULE_INDICATOR_IN_ONTICK,
                "Indicator handle created inside OnTick(). This leaks resources and degrades performance; initialize indicators in OnInit().",
                DiagnosticSeverity::WARNING,
            );
        }
    }

    // Process inline comment suppressions:
    // Supports '// zed-mql-lint: disable <rule-id> [reason]' on either the same line or line directly above.
    let mut suppressions: Vec<(u32, String)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        for caps in re_suppress.captures_iter(line) {
            let rule_id = caps[1].to_string();
            let current = i as u32;
            suppressions.push((current, rule_id.clone()));
            suppressions.push((current + 1, rule_id));
        }
    }

    diagnostics
        .into_iter()
        .filter(|d| {
            let line = d.range.start.line;
            let code = match &d.code {
                Some(NumberOrString::String(s)) => s.as_str(),
                _ => "",
            };
            !suppressions.iter().any(|(l, r)| *l == line && r == code)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_codes(text: &str, config: &Config) -> Vec<String> {
        let uri = Url::parse("file:///test.mq5").unwrap();
        check(text, &uri, config)
            .into_iter()
            .map(|d| match d.code {
                Some(NumberOrString::String(s)) => s,
                _ => String::new(),
            })
            .collect()
    }

    fn check_default(text: &str) -> Vec<String> {
        get_codes(text, &Config::default())
    }

    #[test]
    fn test_rule1_ignored_trade_positive() {
        let src = "void OnTick() {\n   trade.Buy(0.1);\n}";
        assert_eq!(check_default(src), vec![RULE_IGNORED_TRADE]);

        let src_order_send = "void OnTick() {\n   OrderSend(request, result);\n}";
        assert_eq!(check_default(src_order_send), vec![RULE_IGNORED_TRADE]);
    }

    #[test]
    fn test_rule1_ignored_trade_negative() {
        let src = "void OnTick() {\n   bool ok = trade.Buy(0.1);\n   if (ok) return;\n}";
        assert!(!check_default(src).contains(&RULE_IGNORED_TRADE.to_string()));
    }

    #[test]
    fn test_rule1_ignored_trade_suppression() {
        let src_same_line = "void OnTick() {\n   trade.Buy(0.1); // zed-mql-lint: disable ignored-trade-result testing fire-and-forget\n}";
        assert!(!check_default(src_same_line).contains(&RULE_IGNORED_TRADE.to_string()));

        let src_above_line = "void OnTick() {\n   // zed-mql-lint: disable ignored-trade-result testing\n   trade.Buy(0.1);\n}";
        assert!(!check_default(src_above_line).contains(&RULE_IGNORED_TRADE.to_string()));
    }

    #[test]
    fn test_rule2_uninspected_trade_positive() {
        let src =
            "void OnTick() {\n   if (trade.Buy(0.1)) {\n      Print(\"Order submitted\");\n   }\n}";
        assert_eq!(check_default(src), vec![RULE_UNINSPECTED_TRADE]);
    }

    #[test]
    fn test_rule2_uninspected_trade_negative() {
        let src = "void OnTick() {\n   if (trade.Buy(0.1)) {\n      if (trade.ResultRetcode() == TRADE_RETCODE_DONE) {\n         Print(\"Done\");\n      }\n   }\n}";
        assert!(!check_default(src).contains(&RULE_UNINSPECTED_TRADE.to_string()));
    }

    #[test]
    fn test_rule2_uninspected_trade_suppression() {
        let src = "void OnTick() {\n   // zed-mql-lint: disable uninspected-trade-result intentional minimal demo\n   if (trade.Buy(0.1)) {\n      Print(\"Done\");\n   }\n}";
        assert!(!check_default(src).contains(&RULE_UNINSPECTED_TRADE.to_string()));
    }

    #[test]
    fn test_rule3_unchecked_copy_buffer_positive() {
        let src_direct = "void OnTick() {\n   CopyBuffer(h, 0, 0, 3, buf);\n}";
        assert_eq!(check_default(src_direct), vec![RULE_UNCHECKED_COPY_BUFFER]);

        let src_assigned = "void OnTick() {\n   int count = CopyBuffer(h, 0, 0, 3, buf);\n   double x = buf[0];\n}";
        assert_eq!(
            check_default(src_assigned),
            vec![RULE_UNCHECKED_COPY_BUFFER]
        );
    }

    #[test]
    fn test_rule3_unchecked_copy_buffer_negative() {
        let src = "void OnTick() {\n   int count = CopyBuffer(h, 0, 0, 3, buf);\n   if (count <= 0) return;\n   double x = buf[0];\n}";
        assert!(!check_default(src).contains(&RULE_UNCHECKED_COPY_BUFFER.to_string()));
    }

    #[test]
    fn test_rule3_unchecked_copy_buffer_suppression() {
        let src = "void OnTick() {\n   CopyBuffer(h, 0, 0, 3, buf); // zed-mql-lint: disable unchecked-copy-buffer known static size\n}";
        assert!(!check_default(src).contains(&RULE_UNCHECKED_COPY_BUFFER.to_string()));
    }

    #[test]
    fn test_rule4_unchecked_indicator_handle_positive() {
        let src = "int OnInit() {\n   int h = iMA(_Symbol, _Period, 14, 0, MODE_SMA, PRICE_CLOSE);\n   return(INIT_SUCCEEDED);\n}";
        assert_eq!(check_default(src), vec![RULE_UNCHECKED_INDICATOR_HANDLE]);
    }

    #[test]
    fn test_rule4_unchecked_indicator_handle_negative() {
        let src = "int OnInit() {\n   int h = iMA(_Symbol, _Period, 14, 0, MODE_SMA, PRICE_CLOSE);\n   if (h == INVALID_HANDLE) return(INIT_FAILED);\n   return(INIT_SUCCEEDED);\n}";
        assert!(!check_default(src).contains(&RULE_UNCHECKED_INDICATOR_HANDLE.to_string()));
    }

    #[test]
    fn test_rule4_unchecked_indicator_handle_suppression() {
        let src = "int OnInit() {\n   // zed-mql-lint: disable unchecked-indicator-handle mock fixture\n   int h = iMA(_Symbol, _Period, 14, 0, MODE_SMA, PRICE_CLOSE);\n   return(INIT_SUCCEEDED);\n}";
        assert!(!check_default(src).contains(&RULE_UNCHECKED_INDICATOR_HANDLE.to_string()));
    }

    #[test]
    fn test_rule5_indicator_in_ontick_positive() {
        let src = "void OnTick() {\n   int h = iMA(_Symbol, _Period, 14, 0, MODE_SMA, PRICE_CLOSE);\n   if (h == INVALID_HANDLE) return;\n}";
        assert_eq!(check_default(src), vec![RULE_INDICATOR_IN_ONTICK]);
    }

    #[test]
    fn test_rule5_indicator_in_ontick_negative() {
        let src = "int OnInit() {\n   int h = iMA(_Symbol, _Period, 14, 0, MODE_SMA, PRICE_CLOSE);\n   if (h == INVALID_HANDLE) return(INIT_FAILED);\n   return(INIT_SUCCEEDED);\n}";
        assert!(!check_default(src).contains(&RULE_INDICATOR_IN_ONTICK.to_string()));
    }

    #[test]
    fn test_rule5_indicator_in_ontick_suppression() {
        let src = "void OnTick() {\n   // zed-mql-lint: disable indicator-in-ontick benchmarking creation cost\n   int h = iMA(_Symbol, _Period, 14, 0, MODE_SMA, PRICE_CLOSE);\n   if (h == INVALID_HANDLE) return;\n}";
        assert!(!check_default(src).contains(&RULE_INDICATOR_IN_ONTICK.to_string()));
    }

    #[test]
    fn test_rule_controls_disable_and_severity() {
        let src = "void OnTick() {\n   trade.Buy(0.1);\n}";
        let mut rules = HashMap::new();
        rules.insert(
            RULE_IGNORED_TRADE.to_string(),
            RuleConfig {
                enabled: Some(false),
                severity: None,
            },
        );
        let config_disabled = Config {
            enabled: true,
            rules,
        };
        assert_eq!(get_codes(src, &config_disabled), Vec::<String>::new());

        let mut rules2 = HashMap::new();
        rules2.insert(
            RULE_IGNORED_TRADE.to_string(),
            RuleConfig {
                enabled: Some(true),
                severity: Some("error".to_string()),
            },
        );
        let config_err = Config {
            enabled: true,
            rules: rules2,
        };
        let uri = Url::parse("file:///test.mq5").unwrap();
        let diags = check(src, &uri, &config_err);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
    }
}
