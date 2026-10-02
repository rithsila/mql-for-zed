use regex::Regex;
use serde::Deserialize;
use std::{collections::HashMap, fs, path::{Path, PathBuf}};
use walkdir::WalkDir;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind { Function, Class, Enum, Constant, Macro }

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub kind: Kind,
    pub signature: String,
    pub doc: String,
    pub loc: Option<(PathBuf, u32)>,
}

#[derive(Deserialize)]
struct Builtin { name: String, kind: String, signature: String, doc: String }

#[derive(Default)]
pub struct Index {
    pub symbols: HashMap<String, Vec<Symbol>>,
}

const BUILTINS: &str = include_str!("../data/builtins.json");
const NOT_TYPES: &[&str] = &["return", "else", "if", "while", "for", "switch", "new", "delete", "case", "do", "sizeof", "typedef"];

struct Res { func: Regex, class: Regex, en: Regex, def: Regex, enumval: Regex }

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
    p.is_empty() || p == "void" || p == "..." || p.split(',').all(|a| {
        let a = a.split('=').next().unwrap_or("").trim();
        a.split_whitespace().count() >= 2 || a.contains('&') || a.contains('*') || a.contains("[]")
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
                idx.add(Symbol { name: b.name, kind, signature: b.signature, doc: b.doc, loc: None });
            }
        }
        idx
    }

    fn add(&mut self, s: Symbol) {
        self.symbols.entry(s.name.clone()).or_default().push(s);
    }

    pub fn remove_file(&mut self, path: &Path) {
        self.symbols.retain(|_, v| {
            v.retain(|s| s.loc.as_ref().map_or(true, |(p, _)| p != path));
            !v.is_empty()
        });
    }

    pub fn index_dir(&mut self, dir: &Path) {
        for e in WalkDir::new(dir).max_depth(12).into_iter().filter_map(Result::ok) {
            let p = e.path();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
            if e.file_type().is_file() && matches!(ext, "mqh" | "mq5") {
                self.index_file(p);
            }
        }
    }

    pub fn index_file(&mut self, path: &Path) {
        if let Ok(text) = fs::read_to_string(path) {
            self.index_text(path, &text);
        }
    }

    pub fn index_text(&mut self, path: &Path, text: &str) {
        self.remove_file(path);
        thread_local!(static RES: Res = Res::new());
        let mut doc: Vec<String> = vec![];
        let mut in_enum = false;
        let mut depth: i32 = 0;
        let mut found = vec![];
        RES.with(|r| {
            for (i, line) in text.lines().enumerate() {
                let t = line.trim();
                if let Some(c) = t.strip_prefix("//") {
                    let c = c.trim_start_matches('/').trim_matches(|ch: char| ch == '+' || ch == '|' || ch.is_whitespace());
                    if !c.chars().all(|ch| "-=*+|/ ".contains(ch)) { doc.push(c.to_string()); }
                    continue;
                }
                let d = doc.join(" ");
                let at = |name: String, kind, signature: String| Symbol { name, kind, signature, doc: d.clone(), loc: Some((path.to_path_buf(), i as u32)) };
                if in_enum {
                    if t.starts_with('}') { in_enum = false; }
                    else if let Some(c) = r.enumval.captures(line) {
                        found.push(at(c[1].to_string(), Kind::Constant, t.trim_end_matches(',').to_string()));
                    }
                } else if let Some(c) = r.def.captures(line) {
                    let args = c.get(2).map_or("", |m| m.as_str());
                    found.push(at(c[1].to_string(), Kind::Macro, format!("#define {}{} {}", &c[1], args, c[3].trim()).trim().to_string()));
                } else if let Some(c) = r.en.captures(line) {
                    found.push(at(c[1].to_string(), Kind::Enum, format!("enum {}", &c[1])));
                    in_enum = !t.contains('}') && (t.contains('{') || !t.ends_with(';'));
                } else if let Some(c) = r.class.captures(line) {
                    if !t.ends_with(';') {
                        let sig = match c.get(2) { Some(b) => format!("class {} : {}", &c[1], b.as_str()), None => format!("class {}", &c[1]) };
                        found.push(at(c[1].to_string(), Kind::Class, sig));
                    }
                } else if depth <= 1 {
                    if let Some(c) = r.func.captures(line) {
                        let ty = c[1].trim();
                        if !NOT_TYPES.contains(&ty) && params_ok(&c[3]) {
                            found.push(at(c[2].to_string(), Kind::Function, format!("{} {}({})", ty, &c[2], c[3].trim())));
                        }
                    }
                }
                depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
                depth = depth.max(0);
                doc.clear();
            }
        });
        for s in found { self.add(s); }
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
    }
}
