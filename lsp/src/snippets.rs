use lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
};

pub struct Snippet {
    pub label: &'static str,
    pub detail: &'static str,
    pub doc: &'static str,
    pub body: &'static str,
}

pub const SNIPPETS: &[Snippet] = &[
    Snippet {
        label: "OnInit",
        detail: "int OnInit() handler",
        doc: "Expert Advisor initialization event handler skeleton returning INIT_SUCCEEDED.",
        body: "int OnInit()\n{\n   ${1:// Initialization code}\n   return(INIT_SUCCEEDED);\n}",
    },
    Snippet {
        label: "OnTick",
        detail: "void OnTick() handler",
        doc: "Expert Advisor new-tick event handler skeleton.",
        body: "void OnTick()\n{\n   ${1:// Tick handling logic}\n}",
    },
    Snippet {
        label: "OnDeinit",
        detail: "void OnDeinit(const int reason) handler",
        doc: "Expert Advisor deinitialization event handler skeleton.",
        body: "void OnDeinit(const int reason)\n{\n   ${1:// Cleanup code}\n}",
    },
    Snippet {
        label: "ea-skeleton",
        detail: "MQL5 EA lifecycle skeleton",
        doc: "Complete Expert Advisor skeleton with OnInit, OnDeinit, and OnTick handlers.",
        body: "//+------------------------------------------------------------------+\n//| Expert initialization function                                   |\n//+------------------------------------------------------------------+\nint OnInit()\n{\n   ${1:// Initialization code}\n   return(INIT_SUCCEEDED);\n}\n\n//+------------------------------------------------------------------+\n//| Expert deinitialization function                                 |\n//+------------------------------------------------------------------+\nvoid OnDeinit(const int reason)\n{\n   ${2:// Deinitialization code}\n}\n\n//+------------------------------------------------------------------+\n//| Expert tick function                                             |\n//+------------------------------------------------------------------+\nvoid OnTick()\n{\n   ${3:// Tick code}\n}",
    },
    Snippet {
        label: "trade-setup",
        detail: "CTrade instance and setup in OnInit",
        doc: "Safe CTrade setup with magic number, deviation, and filling type.",
        body: "#include <Trade\\Trade.mqh>\n\nCTrade trade;\n\nint OnInit()\n{\n   trade.SetExpertMagicNumber(${1:123456});\n   trade.SetDeviationInPoints(${2:10});\n   trade.SetTypeFilling(ORDER_FILLING_FOK);\n   return(INIT_SUCCEEDED);\n}",
    },
    Snippet {
        label: "indicator-init",
        detail: "Safe indicator initialization and cleanup",
        doc: "Initializes an indicator handle in OnInit, checks for INVALID_HANDLE, and releases it in OnDeinit.",
        body: "int handle_ma = INVALID_HANDLE;\n\nint OnInit()\n{\n   handle_ma = iMA(_Symbol, _Period, ${1:14}, 0, MODE_SMA, PRICE_CLOSE);\n   if(handle_ma == INVALID_HANDLE)\n   {\n      Print(\"Failed to create indicator handle: \", GetLastError());\n      return(INIT_FAILED);\n   }\n   return(INIT_SUCCEEDED);\n}\n\nvoid OnDeinit(const int reason)\n{\n   if(handle_ma != INVALID_HANDLE)\n   {\n      IndicatorRelease(handle_ma);\n      handle_ma = INVALID_HANDLE;\n   }\n}",
    },
    Snippet {
        label: "copy-buffer",
        detail: "Safe CopyBuffer element count check",
        doc: "Sets dynamic array as series, copies buffer, and validates count > 0 before accessing elements.",
        body: "double buffer[];\nArraySetAsSeries(buffer, true);\nint copied = CopyBuffer(${1:handle_ma}, 0, 0, ${2:3}, buffer);\nif(copied <= 0)\n{\n   Print(\"Failed to copy indicator buffer: \", GetLastError());\n   return;\n}\ndouble val = buffer[0];",
    },
    Snippet {
        label: "trade-check",
        detail: "Trade execution with ResultRetcode inspection",
        doc: "Executes trade and inspects actual result return code (TRADE_RETCODE_DONE).",
        body: "if(trade.Buy(${1:0.1}))\n{\n   if(trade.ResultRetcode() == TRADE_RETCODE_DONE)\n   {\n      Print(\"Trade executed successfully. Ticket: \", trade.ResultOrder());\n   }\n   else\n   {\n      Print(\"Order submitted with retcode: \", trade.ResultRetcode(), \" (\", trade.ResultRetcodeDescription(), \")\");\n   }\n}\nelse\n{\n   Print(\"Trade execution failed with retcode: \", trade.ResultRetcode(), \" (\", trade.ResultRetcodeDescription(), \")\");\n}",
    },
    Snippet {
        label: "new-bar",
        detail: "New-bar detection helper",
        doc: "Detects open of a new bar on current symbol and timeframe using iTime.",
        body: "bool IsNewBar()\n{\n   static datetime last_bar_time = 0;\n   datetime current_bar_time = iTime(_Symbol, _Period, 0);\n   if(current_bar_time == 0)\n      return(false);\n\n   if(current_bar_time != last_bar_time)\n   {\n      last_bar_time = current_bar_time;\n      return(true);\n   }\n   return(false);\n}",
    },
    Snippet {
        label: "OnTester",
        detail: "double OnTester() optimization handler",
        doc: "Strategy Tester custom criterion calculation handler.",
        body: "double OnTester()\n{\n   ${1:// Custom optimization criterion}\n   return(0.0);\n}",
    },
];

pub fn all_completion_items() -> Vec<CompletionItem> {
    SNIPPETS
        .iter()
        .map(|s| CompletionItem {
            label: s.label.to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some(s.detail.to_string()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: s.doc.to_string(),
            })),
            insert_text: Some(s.body.to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
pub fn strip_placeholders(snippet: &str) -> String {
    let re_named = regex::Regex::new(r"\$\{\d+:([^}]*)\}").unwrap();
    let re_simple = regex::Regex::new(r"\$\d+").unwrap();
    let intermediate = re_named.replace_all(snippet, "$1");
    re_simple.replace_all(&intermediate, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Index;
    use crate::lint::{self, Config};
    use lsp_types::Url;
    use std::path::Path;

    #[test]
    fn test_snippets_are_non_empty_and_documented() {
        for s in SNIPPETS {
            assert!(!s.label.is_empty());
            assert!(!s.detail.is_empty());
            assert!(!s.doc.is_empty());
            assert!(!s.body.is_empty());
        }
    }

    #[test]
    fn test_snippet_braces_and_brackets_are_balanced() {
        for s in SNIPPETS {
            let expanded = strip_placeholders(s.body);
            let open_braces = expanded.matches('{').count();
            let close_braces = expanded.matches('}').count();
            assert_eq!(
                open_braces, close_braces,
                "Snippet {} has unbalanced braces: open={}, close={}",
                s.label, open_braces, close_braces
            );

            let open_parens = expanded.matches('(').count();
            let close_parens = expanded.matches(')').count();
            assert_eq!(
                open_parens, close_parens,
                "Snippet {} has unbalanced parentheses: open={}, close={}",
                s.label, open_parens, close_parens
            );
        }
    }

    #[test]
    fn test_snippets_pass_lint_cleanly() {
        let uri = Url::parse("file:///snippet_test.mq5").unwrap();
        let config = Config::default();

        for s in SNIPPETS {
            let expanded = strip_placeholders(s.body);
            let diags = lint::check(&expanded, &uri, &config);
            assert!(
                diags.is_empty(),
                "Snippet '{}' produced unexpected lint diagnostics: {:?}",
                s.label,
                diags
            );
        }
    }

    #[test]
    fn test_snippets_indexable() {
        let mut idx = Index::default();
        let path = Path::new("/test/snippet.mq5");
        for s in SNIPPETS {
            let expanded = strip_placeholders(s.body);
            idx.index_text(path, &expanded);
        }
    }
}
