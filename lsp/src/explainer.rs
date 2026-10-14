use crate::hover_config::{HoverConfig, PrimaryLanguage};
use crate::index::{Kind, KnowledgeDoc};
use crate::localization::Catalog;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct AnalyzedStatement {
    pub kind: &'static str,
    pub title: String,
    pub snippet: String,
    pub summary_en: String,
    pub breakdown_en: Vec<String>,
    pub summary_km: String,
    pub gotcha_tip: Option<String>,
}

/// Analyzes an arbitrary MQL5 code snippet or statement into structured components.
pub fn analyze_snippet(code: &str) -> AnalyzedStatement {
    let trimmed = code.trim();

    // 1. Detect structures/classes
    if trimmed.starts_with("struct ") || trimmed.starts_with("class ") {
        return analyze_struct_or_class(trimmed);
    }

    // 2. Detect if conditions
    if trimmed.starts_with("if") || trimmed.contains("&&") || trimmed.contains("||") {
        return analyze_condition(trimmed);
    }

    // 3. Detect loops
    if trimmed.starts_with("for") || trimmed.starts_with("while") {
        return analyze_loop(trimmed);
    }

    // 4. Detect return statements
    if trimmed.starts_with("return") {
        return analyze_return(trimmed);
    }

    // 5. Detect input parameters
    if trimmed.starts_with("input ") || trimmed.starts_with("sinput ") {
        return analyze_input(trimmed);
    }

    // 6. General expression / snippet analysis
    analyze_general(trimmed)
}

fn analyze_struct_or_class(code: &str) -> AnalyzedStatement {
    let re = Regex::new(r"(struct|class)\s+([A-Za-z_]\w*)").unwrap();
    let name = re.captures(code).map_or("CustomObject", |c| {
        c.get(2).map_or("CustomObject", |m| m.as_str())
    });

    let mut breakdown = Vec::new();
    if name.contains("Symbol") || name.contains("Context") {
        breakdown.push("• Tracks market data and trading state for an individual currency pair (spread, lot size, current open trades, profit/loss).".to_string());
        breakdown.push("• In multi-currency or grid EAs, each traded symbol has its own context object so trades don't mix up.".to_string());
    } else if name.contains("Grid") {
        breakdown.push("• Manages the grid of orders (distance between levels, lot sizing multipliers, break-even targets).".to_string());
    } else {
        breakdown.push(format!(
            "• Packages settings and variables into a single organized structure `{}`.",
            name
        ));
    }

    let summary_km = format!(
        "នេះជាទម្រង់រចនាសម្ព័ន្ធ (Struct/Class) ឈ្មោះ `{}` ដែលប្រើសម្រាប់ប្រមូលផ្តុំទិន្នន័យពាក់ព័ន្ធនឹងការជួញដូរ (ដូចជាព័ត៌មានគូរូបិយប័ណ្ណ ការគ្រប់គ្រងតម្លៃ ឬទំហំឡូត៍) ទុកនៅកន្លែងតែមួយ ដើម្បីងាយស្រួលគ្រប់គ្រងក្នុងមនុស្សយន្ត EA។",
        name
    );

    AnalyzedStatement {
        kind: "Class",
        title: "Class/Struct Blueprint".to_string(),
        snippet: code.to_string(),
        summary_en: format!("Defines `{}`—a custom blueprint/container used by this EA to organize related trading information.", name),
        breakdown_en: breakdown,
        summary_km,
        gotcha_tip: Some("Hovering over instances of this struct in code will show its member variables and documentation.".to_string()),
    }
}

fn analyze_condition(code: &str) -> AnalyzedStatement {
    let mut breakdown = Vec::new();

    if code.contains("AllowHedging") {
        breakdown.push("• `AllowHedging`: Setting that controls whether the EA is permitted to hold Buy and Sell orders simultaneously on the same symbol.".to_string());
    }
    if code.contains("ACCOUNT_MARGIN_MODE_RETAIL_HEDGING") {
        breakdown.push("• `ACCOUNT_MARGIN_MODE_RETAIL_HEDGING`: Standard MetaTrader 5 mode allowing independent simultaneous long and short positions.".to_string());
    }
    if code.contains("PositionsTotal") {
        breakdown
            .push("• `PositionsTotal()`: Counts active open trades on the account.".to_string());
    }
    if code.contains("OrdersTotal") {
        breakdown.push(
            "• `OrdersTotal()`: Counts active pending orders (limits and stops).".to_string(),
        );
    }
    if code.contains("TRADE_RETCODE_DONE") {
        breakdown.push("• `TRADE_RETCODE_DONE`: Broker response indicating trade request executed successfully.".to_string());
    }
    if code.contains("&&") {
        breakdown.push("• `&&` (AND): Every condition connected by `&&` must be true for this block to execute.".to_string());
    }
    if code.contains("||") {
        breakdown.push(
            "• `||` (OR): If at least one of the conditions is true, this block will execute."
                .to_string(),
        );
    }
    if code.contains("!=") {
        breakdown.push(
            "• `!=` (NOT EQUAL): Verifies that the left value does not match the right value."
                .to_string(),
        );
    }
    if code.contains("==") {
        breakdown.push("• `==` (EQUAL): Verifies that both values are identical.".to_string());
    }
    if code.contains(">=") {
        breakdown.push("• `>=` (GREATER OR EQUAL): Verifies that the value meets or exceeds the required threshold.".to_string());
    } else if code.contains('>') {
        breakdown.push("• `>` (GREATER THAN): Verifies that the left value is strictly higher than the threshold.".to_string());
    }
    if code.contains("<=") {
        breakdown.push(
            "• `<=` (LESS OR EQUAL): Verifies that the value is at or below the maximum limit."
                .to_string(),
        );
    } else if code.contains('<') {
        breakdown.push(
            "• `<` (LESS THAN): Verifies that the left value is strictly below the limit."
                .to_string(),
        );
    }

    let summary_km = if code.contains("AllowHedging") && code.contains("ACCOUNT_MARGIN_MODE") {
        "លក្ខខណ្ឌនេះពិនិត្យមើលថាតើ EA ត្រូវបានអនុញ្ញាតឱ្យបើកការជួញដូរបែប Hedging (Buy និង Sell ព្រមគ្នា) ឬទេ ហើយពិនិត្យមើលថាតើគណនីឈ្មួញកណ្តាល (Broker) មិនមែនជាប្រភេទ Retail Hedging។ ប្រសិនបើគណនីជាប្រភេទ Netting ឬ Exchange វានឹងដឹងភ្លាមៗដើម្បីបញ្ចៀសកំហុសបដិសេធការជួញដូរ។".to_string()
    } else {
        "នេះជាលក្ខខណ្ឌត្រួតពិនិត្យសុវត្ថិភាព។ EA នឹងធ្វើការផ្ទៀងផ្ទាត់ថាតើលក្ខខណ្ឌទីផ្សារ ឬគណនីត្រឹមត្រូវឬនៅ មុនពេលអនុញ្ញាតឱ្យដំណើរការកូដនៅខាងក្នុង។".to_string()
    };

    let gotcha_tip = if code.contains("ACCOUNT_MARGIN_MODE") {
        Some("⚠️ MetaTrader 5 accounts operate either in Hedging mode or Netting mode. On Netting accounts, opening an opposite trade automatically closes or reduces your existing position instead of opening a second trade!".to_string())
    } else if code.contains("AllowHedging") {
        Some("⚠️ Hedging allows holding Buy and Sell orders simultaneously, but requires adequate account margin for both directions.".to_string())
    } else {
        Some("Make sure all variables checked in this condition are initialized properly before this line runs.".to_string())
    };

    AnalyzedStatement {
        kind: "Condition",
        title: "Condition Gate".to_string(),
        snippet: code.to_string(),
        summary_en: "This is a trading safety/decision gate. The EA checks whether specific account or market conditions are met before taking action.".to_string(),
        breakdown_en: breakdown,
        summary_km,
        gotcha_tip,
    }
}

fn analyze_loop(code: &str) -> AnalyzedStatement {
    let mut breakdown = Vec::new();

    if code.contains("PositionsTotal") {
        breakdown.push("• Loops through all open positions in your account.".to_string());
        if code.contains("i--") || code.contains(">= 0") {
            breakdown.push("• 💡 Best practice detected: Iterating backwards (`i--`) is essential when closing or modifying trades so closing an order doesn't shift the indexes of remaining orders!".to_string());
        }
    } else if code.contains("OrdersTotal") {
        breakdown.push("• Loops through all pending limit and stop orders.".to_string());
    } else {
        breakdown.push("• Iterates across items in an array or list to calculate cumulative values or find specific entries.".to_string());
    }

    AnalyzedStatement {
        kind: "Loop",
        title: "Loop Iteration".to_string(),
        snippet: code.to_string(),
        summary_en: "This is a loop that scans through a collection of items one-by-one (such as scanning all currently open trades or past candles).".to_string(),
        breakdown_en: breakdown,
        summary_km: "នេះជាលូប (Loop) សម្រាប់រត់ត្រួតពិនិត្យបញ្ជីទិន្នន័យម្តងមួយៗ (ដូចជាការដើរពិនិត្យការបញ្ជាទិញដែលកំពុងបើកទាំងអស់ ដើម្បីគណនាប្រាក់ចំណេញសរុប ឬបិទការជួញដូរ)។".to_string(),
        gotcha_tip: Some("Always use reverse loops (`for(int i = PositionsTotal() - 1; i >= 0; i--)`) if your loop intends to close trades!".to_string()),
    }
}

fn analyze_return(code: &str) -> AnalyzedStatement {
    let mut breakdown = Vec::new();
    let (summary_en, summary_km, gotcha) = if code.contains("INIT_SUCCEEDED") {
        breakdown.push("• `INIT_SUCCEEDED` (0): Signals successful EA setup. MetaTrader now begins calling `OnTick()` on every incoming price change.".to_string());
        (
            "Signals to MetaTrader 5 that initialization succeeded and the EA is active.",
            "បញ្ជាក់ប្រាប់ MetaTrader ថា EA បានត្រៀមខ្លួនរួចរាល់ដោយជោគជ័យក្នុងការជួញដូរនៅលើក្រាហ្វ។",
            Some("Once OnInit returns INIT_SUCCEEDED, the EA begins receiving live price ticks and placing trades.".to_string()),
        )
    } else if code.contains("INIT_FAILED") {
        breakdown.push("• `INIT_FAILED` (non-zero): Halts EA execution. MetaTrader unloads the EA from the chart immediately to prevent capital loss.".to_string());
        (
            "Signals an initialization failure. MetaTrader will immediately remove the EA from the chart.",
            "បញ្ជាក់ថាការចាប់ផ្តើម EA បានបរាជ័យ។ MetaTrader នឹងដក EA ចេញពីក្រាហ្វភ្លាមៗដើម្បីសុវត្ថិភាពដើមទុន។",
            Some("Returning INIT_FAILED protects your account when indicator handles fail or account licensing checks fail.".to_string()),
        )
    } else if code.contains("INIT_PARAMETERS_INCORRECT") {
        breakdown.push(
            "• `INIT_PARAMETERS_INCORRECT`: Flags improper inputs in chart settings.".to_string(),
        );
        (
            "Signals invalid user input settings in the Inputs tab. The EA halts execution until parameters are corrected.",
            "បញ្ជាក់ថាការកំណត់ប៉ារ៉ាម៉ែត្រ Inputs មិនត្រឹមត្រូវ។ EA នឹងមិនដំណើរការរហូតទាល់តែកែសម្រួលរួចរាល់។",
            Some("Use this to prevent trading with negative lot sizes or inverted stop-loss values.".to_string()),
        )
    } else {
        (
            "Exits the current function and returns the computed result to the caller.",
            "បញ្ចប់ការដំណើរការអនុគមន៍ និងត្រឡប់លទ្ធផលទៅកាន់កន្លែងដែលបានហៅវា។",
            None,
        )
    };

    AnalyzedStatement {
        kind: "Return",
        title: "Return Statement".to_string(),
        snippet: code.to_string(),
        summary_en: summary_en.to_string(),
        breakdown_en: breakdown,
        summary_km: summary_km.to_string(),
        gotcha_tip: gotcha,
    }
}

fn analyze_input(code: &str) -> AnalyzedStatement {
    let mut breakdown = Vec::new();
    breakdown.push(
        "• Traders can customize this value directly on the chart without modifying any code."
            .to_string(),
    );
    breakdown.push("• It can also be optimized in MetaTrader 5's Strategy Tester.".to_string());

    AnalyzedStatement {
        kind: "Input",
        title: "Input Parameter".to_string(),
        snippet: code.to_string(),
        summary_en: "This is a user-configurable input setting that appears in MetaTrader's EA Settings window (Inputs tab).".to_string(),
        breakdown_en: breakdown,
        summary_km: "នេះជាប៉ារ៉ាម៉ែត្រកំណត់ (Input Parameter) ដែលបង្ហាញនៅលើផ្ទាំង Settings នៃ EA ក្នុង MetaTrader។ អ្នកជួញដូរអាចកែប្រែតម្លៃនេះបានយ៉ាងងាយស្រួលដោយមិនចាំបាច់ប៉ះពាល់កូដឡើយ។".to_string(),
        gotcha_tip: Some("Traders can optimize this setting across historical data using MT5 Strategy Tester.".to_string()),
    }
}

fn analyze_general(code: &str) -> AnalyzedStatement {
    let mut breakdown = Vec::new();
    if code.contains("OrderSend") {
        breakdown.push(
            "• `OrderSend`: Submits a live or pending order to the broker server.".to_string(),
        );
    }
    if code.contains("NormalizeDouble") {
        breakdown.push("• `NormalizeDouble`: Rounds prices or lot sizes to the exact precision required by your broker.".to_string());
    }
    if code.contains("CopyBuffer") {
        breakdown.push(
            "• `CopyBuffer`: Reads indicator values into an array for Buy/Sell decisions."
                .to_string(),
        );
    }
    if code.contains("CopyRates") {
        breakdown.push(
            "• `CopyRates`: Copies historical candlestick prices (OHLC) for market analysis."
                .to_string(),
        );
    }
    if code.contains("SymbolInfoDouble") {
        breakdown.push("• `SymbolInfoDouble`: Queries market prices such as current Bid, Ask, or minimum lot step.".to_string());
    }

    AnalyzedStatement {
        kind: "Statement",
        title: "Trading Statement".to_string(),
        snippet: code.to_string(),
        summary_en: "This code performs a trading calculation, variable assignment, or function call in your Expert Advisor.".to_string(),
        breakdown_en: breakdown,
        summary_km: "កូដនេះធ្វើការគណនា ឬហៅប្រើមុខងារជួញដូរនៅក្នុងមនុស្សយន្ត EA។ សូមពិនិត្យមើលតម្លៃអថេរ និងលទ្ធផលត្រឡប់ដើម្បីប្រាកដថាដំណើរការដោយគ្មានកំហុស។".to_string(),
        gotcha_tip: None,
    }
}

/// Renders a concise, Markdown-formatted hover explainer tailored for the editor hover popup.
pub fn explain_statement_hover(code: &str, config: &HoverConfig) -> Option<String> {
    if !config.enabled {
        return None;
    }
    let analyzed = analyze_snippet(code);
    let primary = config.effective_primary();
    let mut out = String::new();

    // 1. Title and Kind
    let kind_label = Catalog::kind_label(primary, analyzed.kind);
    let explainer_title = Catalog::vibe_explainer_heading(primary);
    out.push_str(&format!(
        "**{}** · {} ({})\n\n",
        explainer_title, kind_label, analyzed.title
    ));

    // 2. Code snippet
    out.push_str("```mql5\n");
    out.push_str(&analyzed.snippet);
    out.push_str("\n```");

    if !config.show_documentation || config.is_facts_only() {
        return Some(out);
    }

    out.push_str("\n\n");

    let render_en = || -> String {
        let mut en = format!(
            "**{}**\n\n{}",
            Catalog::language_heading(PrimaryLanguage::English),
            analyzed.summary_en
        );
        if !analyzed.breakdown_en.is_empty() {
            en.push_str("\n\n");
            en.push_str(&analyzed.breakdown_en.join("\n"));
        }
        en
    };

    let render_km = || -> String {
        format!(
            "**{}**\n\n{}",
            Catalog::language_heading(PrimaryLanguage::Khmer),
            analyzed.summary_km
        )
    };

    let mut sections = Vec::new();
    if config.is_bilingual() {
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
    } else if config.languages.english {
        let mut en = analyzed.summary_en.clone();
        if !analyzed.breakdown_en.is_empty() {
            en.push_str("\n\n");
            en.push_str(&analyzed.breakdown_en.join("\n"));
        }
        sections.push(en);
    } else if config.languages.khmer {
        sections.push(analyzed.summary_km.clone());
    }

    if let Some(gotcha) = &analyzed.gotcha_tip {
        let label = Catalog::trader_gotcha_label(primary);
        sections.push(format!("💡 **{}**\n\n{}", label, gotcha));
    }

    if !sections.is_empty() {
        out.push_str(&sections.join("\n\n"));
    }

    Some(out)
}

/// Synthesize an intelligent heuristic Vibe Check for custom symbols without manual documentation.
pub fn heuristic_vibe(name: &str, kind: Kind) -> Option<String> {
    if kind == Kind::Class {
        if name.contains("Symbol") || name.contains("Context") {
            Some(format!(
                "Holds quotes, lot calculations, and trade records for currency `{}`.",
                name
            ))
        } else if name.contains("Grid") {
            Some(format!(
                "Manages order grid distance, lot multipliers, and basket break-even targets for `{}`.",
                name
            ))
        } else if name.contains("Trailing") {
            Some(
                "Dynamically trails stop-loss levels to lock in profits as market moves favorably."
                    .to_string(),
            )
        } else {
            Some(format!(
                "Blueprint container for `{}` in this trading system.",
                name
            ))
        }
    } else if kind == Kind::Function {
        if name.starts_with("Check") || name.contains("Allow") || name.contains("Valid") {
            Some(
                "Gatekeeper function validating market or account conditions before trade execution."
                    .to_string(),
            )
        } else if name.contains("Open") || name.contains("Buy") || name.contains("Sell") {
            Some(
                "Order execution routine placing or staging new trades on the broker server."
                    .to_string(),
            )
        } else if name.contains("Close") {
            Some("Exit logic closing open trades or clearing pending orders.".to_string())
        } else if name.contains("Lot") || name.contains("Risk") {
            Some("Risk and money management routine calculating safe position volume.".to_string())
        } else {
            None
        }
    } else {
        None
    }
}

/// Analyzes an arbitrary MQL5 code snippet or symbol and generates an intuitive,
/// trader-focused explanation in plain English and Khmer for CLI output.
pub fn explain_code(code: &str) -> String {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return "⚠️ No code selected. Please highlight an MQL5 code snippet in Zed and run again."
            .to_string();
    }

    let analyzed = analyze_snippet(trimmed);

    let mut out = String::new();
    out.push_str("══════════════════════════════════════════════════════════════\n");
    out.push_str(" 🤖 MQL5 Vibe Explainer (Trader & Non-Coder Edition)\n");
    out.push_str("══════════════════════════════════════════════════════════════\n\n");

    out.push_str("📌 Analyzed Code:\n");
    out.push_str("```mql5\n");
    out.push_str(&analyzed.snippet);
    out.push_str("\n```\n\n");

    out.push_str("🎯 The Big Picture (Vibe Summary):\n");
    out.push_str(&analyzed.summary_en);
    out.push_str("\n\n");

    if !analyzed.breakdown_en.is_empty() {
        out.push_str("🔍 Breakdown:\n");
        out.push_str(&analyzed.breakdown_en.join("\n"));
        out.push_str("\n\n");
    }

    out.push_str("🇰🇭 ការពន្យល់ជាភាសាខ្មែរ (Khmer Explanation):\n");
    out.push_str(&analyzed.summary_km);
    out.push_str("\n\n");

    if let Some(gotcha) = &analyzed.gotcha_tip {
        out.push_str("💡 Trader Gotcha / Risk Tip:\n");
        out.push_str(gotcha);
        out.push_str("\n\n");
    }

    out.push_str("══════════════════════════════════════════════════════════════\n");
    out
}

/// Scans an MQL5 file, extracts custom structs, classes, inputs, and functions,
/// and returns generated knowledge entries.
pub fn generate_file_knowledge(content: &str) -> HashMap<String, KnowledgeDoc> {
    let mut map = HashMap::new();

    // Regex for struct/class
    let class_re = Regex::new(r"^\s*(?:class|struct)\s+([A-Za-z_]\w*)(?:\s*:\s*(?:public\s+|protected\s+|private\s+)?([A-Za-z_]\w*))?\s*(\{|$)").unwrap();
    // Regex for input
    let input_re =
        Regex::new(r"^\s*(?:input|sinput)\s+([A-Za-z_]\w*)\s+([A-Za-z_]\w*)\s*=\s*([^;]+);")
            .unwrap();
    // Regex for functions
    let func_re = Regex::new(r"^\s*(?:(?:virtual|static|inline|extern|const|public:|protected:|private:)\s+)*([A-Za-z_][\w:<>]*[\s\*&]+)(~?[A-Za-z_]\w*)\s*\(([^;{)]*)\)").unwrap();

    for line in content.lines() {
        if let Some(c) = class_re.captures(line) {
            let name = c[1].to_string();
            let summary = format!(
                "Custom struct/class `{}` organizing trading context, parameters, or order state.",
                name
            );
            let summary_km = format!("រចនាសម្ព័ន្ធ `{}` សម្រាប់ផ្ទុកទិន្នន័យបរិបទនៃការជួញដូរ និងការកំណត់ក្នុង EA។", name);
            let vibe = if name.contains("Symbol") || name.contains("Context") {
                format!(
                    "Holds all quotes, lot calculations, and trade records for currency `{}`.",
                    name
                )
            } else {
                format!("Blueprint container for `{}` in this trading system.", name)
            };
            map.insert(
                name,
                KnowledgeDoc {
                    en: Some(summary),
                    km: Some(summary_km),
                    vibe: Some(vibe),
                },
            );
        } else if let Some(c) = input_re.captures(line) {
            let ty = c[1].to_string();
            let name = c[2].to_string();
            let default_val = c[3].trim().to_string();
            let summary = format!("User input setting `{}` (type `{}` with default `{}`). Configurable in MT5 Inputs tab.", name, ty, default_val);
            let summary_km = format!(
                "ការកំណត់ `{}` (ប្រភេទ `{}` តម្លៃដើម `{}`) ដែលអាចកែប្រែបានក្នុងផ្ទាំង Inputs។",
                name, ty, default_val
            );
            let vibe = format!(
                "Traders adjust this setting `{}` to tweak risk, lot size, or strategy timing.",
                name
            );
            map.insert(
                name,
                KnowledgeDoc {
                    en: Some(summary),
                    km: Some(summary_km),
                    vibe: Some(vibe),
                },
            );
        } else if let Some(c) = func_re.captures(line) {
            let ret_ty = c[1].trim().to_string();
            let name = c[2].to_string();
            if !name.starts_with("On") && !name.starts_with("~") && name.len() > 2 {
                let summary = format!("Custom trading function `{}` returning `{}`.", name, ret_ty);
                let summary_km = format!("អនុគមន៍ផ្ទាល់ខ្លួន `{}` ដែលត្រឡប់តម្លៃ `{}`។", name, ret_ty);
                let vibe = format!("Executes EA sub-routine for `{}`.", name);
                map.insert(
                    name,
                    KnowledgeDoc {
                        en: Some(summary),
                        km: Some(summary_km),
                        vibe: Some(vibe),
                    },
                );
            }
        }
    }

    map
}

/// Writes or merges knowledge entries into `<workspace>/.mql/knowledge.json`.
pub fn write_workspace_knowledge(
    workspace: &Path,
    new_entries: HashMap<String, KnowledgeDoc>,
) -> std::io::Result<usize> {
    let mql_dir = workspace.join(".mql");
    if !mql_dir.exists() {
        fs::create_dir_all(&mql_dir)?;
    }
    let knowledge_file = mql_dir.join("knowledge.json");

    let mut existing: HashMap<String, KnowledgeDoc> = if knowledge_file.exists() {
        if let Ok(text) = fs::read_to_string(&knowledge_file) {
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };

    let count = new_entries.len();
    for (k, v) in new_entries {
        existing.insert(k, v);
    }

    let json = serde_json::to_string_pretty(&existing)?;
    fs::write(&knowledge_file, json)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explain_condition() {
        let snippet = "if(AllowHedging && mm != ACCOUNT_MARGIN_MODE_RETAIL_HEDGING)";
        let result = explain_code(snippet);
        assert!(result.contains("AllowHedging"));
        assert!(result.contains("ACCOUNT_MARGIN_MODE_RETAIL_HEDGING"));
        assert!(result.contains("ការពន្យល់ជាភាសាខ្មែរ"));
        assert!(result.contains("Netting"));
    }

    #[test]
    fn test_explain_struct() {
        let snippet = "struct SymbolContext\n{\n   string name;\n   double bid;\n};";
        let result = explain_code(snippet);
        assert!(result.contains("SymbolContext"));
        assert!(result.contains("The Big Picture"));
        assert!(result.contains("ការពន្យល់ជាភាសាខ្មែរ"));
    }

    #[test]
    fn test_generate_file_knowledge() {
        let code = "struct SymbolContext\n{\n   string symbol;\n};\ninput double InpLotSize = 0.01;\nvoid ProcessGrid()\n{\n}\n";
        let map = generate_file_knowledge(code);
        assert!(map.contains_key("SymbolContext"));
        assert!(map.contains_key("InpLotSize"));
        assert!(map.contains_key("ProcessGrid"));
        assert!(map["SymbolContext"]
            .en
            .as_ref()
            .unwrap()
            .contains("SymbolContext"));
    }
}
