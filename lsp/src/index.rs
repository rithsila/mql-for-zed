use crate::localization::{parse_comments, LocalizedDoc};
use regex::Regex;
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::RwLock,
};
use walkdir::WalkDir;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Function,
    Class,
    Enum,
    Constant,
    Macro,
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub kind: Kind,
    pub signature: String,
    pub doc: String,
    pub localized_doc: LocalizedDoc,
    pub loc: Option<(PathBuf, u32)>,
}

#[derive(Deserialize)]
struct Builtin {
    name: String,
    kind: String,
    signature: String,
    doc: String,
    #[serde(default)]
    doc_km: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, serde::Serialize)]
pub struct KnowledgeDoc {
    #[serde(default)]
    pub en: Option<String>,
    #[serde(default)]
    pub km: Option<String>,
    #[serde(default)]
    pub vibe: Option<String>,
}

pub fn parse_knowledge_json(text: &str) -> Option<HashMap<String, KnowledgeDoc>> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let mut map = HashMap::new();

    let target = if let Some(syms) = v.get("symbols").and_then(|s| s.as_object()) {
        syms
    } else if let Some(obj) = v.as_object() {
        obj
    } else {
        return None;
    };

    for (k, val) in target {
        if k == "$schema" || k == "version" {
            continue;
        }
        if let Some(s) = val.as_str() {
            map.insert(
                k.clone(),
                KnowledgeDoc {
                    en: Some(s.to_string()),
                    km: None,
                    vibe: None,
                },
            );
        } else if let Some(o) = val.as_object() {
            let en = o
                .get("en")
                .or_else(|| o.get("summary"))
                .or_else(|| o.get("doc"))
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            let km = o
                .get("km")
                .or_else(|| o.get("summary_km"))
                .or_else(|| o.get("doc_km"))
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            let vibe = o
                .get("vibe")
                .or_else(|| o.get("trading_context"))
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            map.insert(k.clone(), KnowledgeDoc { en, km, vibe });
        }
    }
    Some(map)
}

#[derive(Default)]
pub struct Index {
    pub symbols: HashMap<String, Vec<Symbol>>,
    pub knowledge: RwLock<HashMap<String, KnowledgeDoc>>,
    pub workspace_root: Option<PathBuf>,
    pub last_knowledge_mtime: RwLock<Option<std::time::SystemTime>>,
}

const BUILTINS: &str = include_str!("../data/builtins.json");
pub const NOT_TYPES: &[&str] = &[
    "return", "else", "if", "while", "for", "switch", "new", "delete", "case", "do", "sizeof",
    "typedef",
];

struct Res {
    func: Regex,
    class: Regex,
    en: Regex,
    def: Regex,
    enumval: Regex,
}

impl Res {
    fn new() -> Self {
        Res {
            func: Regex::new(r"^\s*(?:(?:virtual|static|inline|extern|const|public:|protected:|private:)\s+)*([A-Za-z_][\w:<>]*[\s\*&]+)(~?[A-Za-z_]\w*)\s*\(([^;{)]*)\)").unwrap(),
            class: Regex::new(r"^\s*(?:class|struct)\s+([A-Za-z_]\w*)(?:\s*:\s*(?:public\s+|protected\s+|private\s+)?([A-Za-z_]\w*))?\s*(\{|$)").unwrap(),
            en: Regex::new(r"^\s*enum\s+([A-Za-z_]\w*)").unwrap(),
            def: Regex::new(r"^\s*#define\s+([A-Za-z_]\w*)(\([^)]*\))?\s*(.*)$").unwrap(),
            enumval: Regex::new(r"^\s*([A-Za-z_]\w*)\s*(=[^,/]*)?\s*,?\s*(//.*)?$").unwrap(),
        }
    }
}

fn params_ok(p: &str) -> bool {
    let p = p.trim();
    p.is_empty()
        || p == "void"
        || p == "..."
        || p.split(',').all(|a| {
            let a = a.split('=').next().unwrap_or("").trim();
            a.split_whitespace().count() >= 2
                || a.contains('&')
                || a.contains('*')
                || a.contains("[]")
        })
}

impl Index {
    pub fn new() -> Self {
        let mut idx = Index::default();
        if let Ok(list) = serde_json::from_str::<Vec<Builtin>>(BUILTINS) {
            for b in list {
                let kind = match b.kind.as_str() {
                    "function" => Kind::Function,
                    "struct" => Kind::Class,
                    "enum" => Kind::Enum,
                    _ => Kind::Constant,
                };
                let localized_doc = LocalizedDoc {
                    en: if b.doc.is_empty() {
                        None
                    } else {
                        Some(b.doc.clone())
                    },
                    km: b.doc_km,
                    unclassified: None,
                    vibe: None,
                    raw: String::new(),
                };
                idx.add(Symbol {
                    name: b.name,
                    kind,
                    signature: b.signature,
                    doc: b.doc,
                    localized_doc,
                    loc: None,
                });
            }
        }
        idx
    }

    fn add(&mut self, s: Symbol) {
        self.symbols.entry(s.name.clone()).or_default().push(s);
    }

    pub fn set_workspace(&mut self, root: PathBuf) {
        self.workspace_root = Some(root);
        self.reload_knowledge();
    }

    pub fn reload_knowledge(&self) {
        if let Some(root) = &self.workspace_root {
            let path = root.join(".mql/knowledge.json");
            if let Ok(metadata) = fs::metadata(&path) {
                if let Ok(mtime) = metadata.modified() {
                    let needs_reload = self
                        .last_knowledge_mtime
                        .read()
                        .map(|guard| guard.map_or(true, |last| mtime > last))
                        .unwrap_or(true);

                    if needs_reload {
                        if let Ok(text) = fs::read_to_string(&path) {
                            if let Some(loaded) = parse_knowledge_json(&text) {
                                if let Ok(mut k_guard) = self.knowledge.write() {
                                    *k_guard = loaded;
                                }
                                if let Ok(mut m_guard) = self.last_knowledge_mtime.write() {
                                    *m_guard = Some(mtime);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn check_reload_knowledge(&self) {
        self.reload_knowledge();
    }

    pub fn symbols_for_hover(&self, word: &str) -> Option<Vec<Symbol>> {
        self.check_reload_knowledge();
        let knowledge_entry = self
            .knowledge
            .read()
            .ok()
            .and_then(|k| k.get(word).cloned());

        if let Some(symbols) = self.symbols.get(word) {
            let mut list = symbols.clone();
            if let Some(k) = knowledge_entry {
                for s in &mut list {
                    if s.localized_doc.is_empty() {
                        s.localized_doc.en = k.en.clone();
                        s.localized_doc.km = k.km.clone();
                        s.doc = k.en.clone().unwrap_or_default();
                    }
                    if s.localized_doc.vibe.is_none() {
                        s.localized_doc.vibe = k.vibe.clone();
                    }
                }
            }
            Some(list)
        } else if let Some(k) = knowledge_entry {
            Some(vec![Symbol {
                name: word.to_string(),
                kind: Kind::Constant,
                signature: format!("// Workspace Knowledge: {}", word),
                doc: k.en.clone().unwrap_or_default(),
                localized_doc: LocalizedDoc {
                    en: k.en,
                    km: k.km,
                    unclassified: None,
                    vibe: k.vibe,
                    raw: String::new(),
                },
                loc: None,
            }])
        } else {
            None
        }
    }

    pub fn remove_file(&mut self, path: &Path) {
        let norm = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.symbols.retain(|_, v| {
            v.retain(|s| {
                s.loc.as_ref().map_or(true, |(p, _)| {
                    let p_norm = p.canonicalize().unwrap_or_else(|_| p.clone());
                    p_norm != norm && p != path && p != &norm
                })
            });
            !v.is_empty()
        });
    }

    pub fn index_dir(&mut self, dir: &Path) {
        for e in WalkDir::new(dir)
            .max_depth(12)
            .into_iter()
            .filter_map(Result::ok)
        {
            let p = e.path();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
            if e.file_type().is_file() && matches!(ext, "mqh" | "mq5") {
                self.index_file(p);
            }
        }
    }

    pub fn index_file(&mut self, path: &Path) {
        let norm = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Ok(text) = fs::read_to_string(&norm) {
            self.index_text(&norm, &text);
        }
    }

    pub fn index_text(&mut self, path: &Path, text: &str) {
        let norm = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.remove_file(&norm);
        thread_local!(static RES: Res = Res::new());
        let mut doc: Vec<String> = vec![];
        let mut in_enum = false;
        let mut depth: i32 = 0;
        let mut found = vec![];
        RES.with(|r| {
            for (i, line) in text.lines().enumerate() {
                let t = line.trim();
                if let Some(c) = t.strip_prefix("//") {
                    let c = c
                        .trim_start_matches('/')
                        .trim_matches(|ch: char| ch == '+' || ch == '|' || ch.is_whitespace());
                    if c.is_empty() {
                        if !doc.is_empty() {
                            doc.push(String::new());
                        }
                    } else if !c.chars().all(|ch| "-=*+|/ ".contains(ch)) {
                        doc.push(c.to_string());
                    }
                    continue;
                }
                let has_tags = doc.iter().any(|line| {
                    let t = line.trim();
                    t.starts_with("@en") || t.starts_with("@km")
                });
                let loc_doc = parse_comments(&doc);
                let d = if has_tags {
                    loc_doc.primary_text()
                } else {
                    doc.join(" ")
                };
                let at = |name: String, kind, signature: String| Symbol {
                    name,
                    kind,
                    signature,
                    doc: d.clone(),
                    localized_doc: loc_doc.clone(),
                    loc: Some((norm.clone(), i as u32)),
                };
                if in_enum {
                    if t.starts_with('}') {
                        in_enum = false;
                    } else if let Some(c) = r.enumval.captures(line) {
                        found.push(at(
                            c[1].to_string(),
                            Kind::Constant,
                            t.trim_end_matches(',').to_string(),
                        ));
                    }
                } else if let Some(c) = r.def.captures(line) {
                    let args = c.get(2).map_or("", |m| m.as_str());
                    found.push(at(
                        c[1].to_string(),
                        Kind::Macro,
                        format!("#define {}{} {}", &c[1], args, c[3].trim())
                            .trim()
                            .to_string(),
                    ));
                } else if let Some(c) = r.en.captures(line) {
                    found.push(at(c[1].to_string(), Kind::Enum, format!("enum {}", &c[1])));
                    in_enum = !t.contains('}') && (t.contains('{') || !t.ends_with(';'));
                } else if let Some(c) = r.class.captures(line) {
                    if !t.ends_with(';') {
                        let sig = match c.get(2) {
                            Some(b) => format!("class {} : {}", &c[1], b.as_str()),
                            None => format!("class {}", &c[1]),
                        };
                        found.push(at(c[1].to_string(), Kind::Class, sig));
                    }
                } else if depth <= 1 {
                    if let Some(c) = r.func.captures(line) {
                        let ty = c[1].trim();
                        if !NOT_TYPES.contains(&ty) && params_ok(&c[3]) {
                            found.push(at(
                                c[2].to_string(),
                                Kind::Function,
                                format!("{} {}({})", ty, &c[2], c[3].trim()),
                            ));
                        }
                    }
                }
                depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
                depth = depth.max(0);
                doc.clear();
            }
        });
        for s in found {
            self.add(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_declarations() {
        let mut i = Index::new();
        let src = "// Opens a buy.\nclass CTrade : public CObject\n{\npublic:\n   bool Buy(const double volume, const string symbol=NULL);\n   virtual int Type() const { return 1; }\n};\nenum E_X\n{\n   E_A=1,\n   E_B,\n};\n#define MAGIC 42\nint Foo(int a)\n{\n   if (a) return Bar(a);\n   CTrade t(5);\n}\n";
        i.index_text(Path::new("/x/a.mqh"), src);
        for n in ["CTrade", "Buy", "Type", "E_X", "E_A", "E_B", "MAGIC", "Foo"] {
            assert!(i.symbols.contains_key(n), "missing {n}");
        }
        assert!(!i.symbols.contains_key("Bar") && !i.symbols.contains_key("t"));
        assert!(i.symbols["CTrade"][0].doc.contains("Opens a buy"));
        let mut j = Index::default();
        j.index_text(Path::new("/x/b.mqh"), "//+------------------+\n//| Class CFoo.      |\n//| Derives from X.  |\n//+------------------+\nclass CFoo : public X\n{\n};\n");
        assert_eq!(j.symbols["CFoo"][0].doc, "Class CFoo. Derives from X.");
        assert!(i.symbols.contains_key("OrderSend"));
        assert!(i.symbols["OrderSend"][0].localized_doc.km.is_some());
    }

    #[test]
    fn indexes_bilingual_comments() {
        let mut i = Index::default();
        let src = "// @en Risk multiplier.\n// @km មេគុណហានិភ័យ។\n#define RISK_MULT 2.5\n";
        i.index_text(Path::new("/x/risk.mqh"), src);
        let s = &i.symbols["RISK_MULT"][0];
        assert_eq!(s.localized_doc.en.as_deref(), Some("Risk multiplier."));
        assert_eq!(s.localized_doc.km.as_deref(), Some("មេគុណហានិភ័យ។"));
        assert_eq!(s.doc, "Risk multiplier.");
    }
}
