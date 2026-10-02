mod index;

use index::{Index, Kind, Symbol};
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{notification::{Notification as _, *}, request::{Request as _, *}, *};
use std::{collections::HashMap, error::Error, fs, path::{Path, PathBuf}};

fn mql5_root(workspace: &Path) -> Option<PathBuf> {
    let mut cur = Some(workspace);
    while let Some(p) = cur {
        if p.file_name().map_or(false, |n| n.eq_ignore_ascii_case("MQL5")) { return Some(p.to_path_buf()); }
        cur = p.parent();
    }
    let c = workspace.join("MQL5");
    c.is_dir().then_some(c)
}

fn default_mql5_roots() -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(p) = std::env::var_os("MQL5_PATH") { v.push(PathBuf::from(p)); }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        v.push(home.join(".config/zed-mql/MQL5"));
        v.push(home.join("Library/Application Support/net.metaquotes.wine.metatrader5/drive_c/Program Files/MetaTrader 5/MQL5"));
        v.push(home.join(".wine/drive_c/Program Files/MetaTrader 5/MQL5"));
    }
    v.push(PathBuf::from("C:/Program Files/MetaTrader 5/MQL5"));
    v
}

fn word_at(text: &str, pos: Position) -> Option<String> {
    let line = text.lines().nth(pos.line as usize)?;
    let chars: Vec<char> = line.chars().collect();
    let is_w = |c: char| c.is_alphanumeric() || c == '_';
    let mut s = (pos.character as usize).min(chars.len());
    let mut e = s;
    while s > 0 && is_w(chars[s - 1]) { s -= 1; }
    while e < chars.len() && is_w(chars[e]) { e += 1; }
    (s < e).then(|| chars[s..e].iter().collect())
}

fn completion_kind(k: Kind) -> CompletionItemKind {
    match k {
        Kind::Function => CompletionItemKind::FUNCTION,
        Kind::Class => CompletionItemKind::CLASS,
        Kind::Enum => CompletionItemKind::ENUM,
        Kind::Constant => CompletionItemKind::CONSTANT,
        Kind::Macro => CompletionItemKind::CONSTANT,
    }
}

fn markdown(s: &Symbol) -> String {
    let mut out = format!("```mql5\n{}\n```", s.signature);
    if !s.doc.is_empty() { out.push_str("\n\n"); out.push_str(&s.doc); }
    out
}

fn path_of(uri: &Url) -> Option<PathBuf> { uri.to_file_path().ok() }

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (conn, io) = Connection::stdio();
    let caps = serde_json::to_value(ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions::default()),
        ..Default::default()
    })?;
    let init: InitializeParams = serde_json::from_value(conn.initialize(caps)?)?;

    let mut index = Index::new();
    let workspace = init.workspace_folders.as_ref().and_then(|f| f.first()).and_then(|f| path_of(&f.uri))
        .or_else(|| init.root_uri.as_ref().and_then(path_of));
    let configured = init.initialization_options.as_ref().and_then(|o| o.get("mql5Path")).and_then(|v| v.as_str()).map(PathBuf::from);
    if let Some(root) = configured.or_else(|| workspace.as_deref().and_then(mql5_root))
        .or_else(|| default_mql5_roots().into_iter().find(|p| p.join("Include").is_dir()))
    {
        index.index_dir(&root.join("Include"));
    }
    if let Some(w) = &workspace { index.index_dir(w); }

    let mut docs: HashMap<Url, String> = HashMap::new();
    for msg in &conn.receiver {
        match msg {
            Message::Request(req) => {
                if conn.handle_shutdown(&req)? { break; }
                let resp = handle_request(req, &docs, &index);
                conn.sender.send(Message::Response(resp))?;
            }
            Message::Notification(n) => handle_notification(n, &mut docs, &mut index),
            Message::Response(_) => {}
        }
    }
    io.join()?;
    Ok(())
}

fn text_of(docs: &HashMap<Url, String>, uri: &Url) -> Option<String> {
    docs.get(uri).cloned().or_else(|| fs::read_to_string(path_of(uri)?).ok())
}

fn handle_request(req: Request, docs: &HashMap<Url, String>, index: &Index) -> Response {
    let id = req.id.clone();
    let ok = |v: serde_json::Value| Response { id: id.clone(), result: Some(v), error: None };
    match req.method.as_str() {
        HoverRequest::METHOD => {
            let p: HoverParams = serde_json::from_value(req.params).unwrap();
            let tdp = p.text_document_position_params;
            let hit = text_of(docs, &tdp.text_document.uri).and_then(|t| word_at(&t, tdp.position)).and_then(|w| index.symbols.get(&w));
            ok(match hit {
                Some(v) => serde_json::to_value(Hover {
                    contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: v.iter().take(5).map(markdown).collect::<Vec<_>>().join("\n\n---\n\n") }),
                    range: None,
                }).unwrap(),
                None => serde_json::Value::Null,
            })
        }
        GotoDefinition::METHOD => {
            let p: GotoDefinitionParams = serde_json::from_value(req.params).unwrap();
            let tdp = p.text_document_position_params;
            let locs: Vec<Location> = text_of(docs, &tdp.text_document.uri)
                .and_then(|t| word_at(&t, tdp.position))
                .and_then(|w| index.symbols.get(&w))
                .map(|v| v.iter().filter_map(|s| {
                    let (path, line) = s.loc.as_ref()?;
                    let pos = Position::new(*line, 0);
                    Some(Location::new(Url::from_file_path(path).ok()?, Range::new(pos, pos)))
                }).collect())
                .unwrap_or_default();
            ok(serde_json::to_value(locs).unwrap())
        }
        Completion::METHOD => {
            let items: Vec<CompletionItem> = index.symbols.values().filter_map(|v| v.first()).map(|s| CompletionItem {
                label: s.name.clone(),
                kind: Some(completion_kind(s.kind)),
                detail: Some(s.signature.clone()),
                documentation: (!s.doc.is_empty()).then(|| Documentation::MarkupContent(MarkupContent { kind: MarkupKind::Markdown, value: s.doc.clone() })),
                ..Default::default()
            }).collect();
            ok(serde_json::to_value(CompletionResponse::Array(items)).unwrap())
        }
        _ => Response { id, result: Some(serde_json::Value::Null), error: None },
    }
}

fn handle_notification(n: Notification, docs: &mut HashMap<Url, String>, index: &mut Index) {
    match n.method.as_str() {
        DidOpenTextDocument::METHOD => {
            if let Ok(p) = serde_json::from_value::<DidOpenTextDocumentParams>(n.params) {
                if let Some(path) = path_of(&p.text_document.uri) { index.index_text(&path, &p.text_document.text); }
                docs.insert(p.text_document.uri, p.text_document.text);
            }
        }
        DidChangeTextDocument::METHOD => {
            if let Ok(p) = serde_json::from_value::<DidChangeTextDocumentParams>(n.params) {
                if let Some(c) = p.content_changes.into_iter().last() {
                    if let Some(path) = path_of(&p.text_document.uri) { index.index_text(&path, &c.text); }
                    docs.insert(p.text_document.uri, c.text);
                }
            }
        }
        DidCloseTextDocument::METHOD => {
            if let Ok(p) = serde_json::from_value::<DidCloseTextDocumentParams>(n.params) { docs.remove(&p.text_document.uri); }
        }
        _ => {}
    }
}
