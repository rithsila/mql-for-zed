mod compiler;
mod index;

use index::{Index, Kind, Symbol};
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    notification::{Notification as _, *},
    request::{Request as _, *},
    *,
};
use std::{
    collections::HashMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
};

fn mql5_root(workspace: &Path) -> Option<PathBuf> {
    let mut cur = Some(workspace);
    while let Some(p) = cur {
        if p.file_name()
            .map_or(false, |n| n.eq_ignore_ascii_case("MQL5"))
        {
            return Some(p.to_path_buf());
        }
        cur = p.parent();
    }
    let c = workspace.join("MQL5");
    c.is_dir().then_some(c)
}

fn default_mql5_roots() -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(p) = std::env::var_os("MQL5_PATH") {
        v.push(PathBuf::from(p));
    }
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
    while s > 0 && is_w(chars[s - 1]) {
        s -= 1;
    }
    while e < chars.len() && is_w(chars[e]) {
        e += 1;
    }
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
    if !s.doc.is_empty() {
        out.push_str("\n\n");
        out.push_str(&s.doc);
    }
    out
}

fn path_of(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (conn, io) = Connection::stdio();
    let caps = serde_json::to_value(ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                ..Default::default()
            },
        )),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions::default()),
        ..Default::default()
    })?;
    let init: InitializeParams = serde_json::from_value(conn.initialize(caps)?)?;

    let mut index = Index::new();
    let workspace = init
        .workspace_folders
        .as_ref()
        .and_then(|f| f.first())
        .and_then(|f| path_of(&f.uri))
        .or_else(|| init.root_uri.as_ref().and_then(path_of));
    let configured = init
        .initialization_options
        .as_ref()
        .and_then(|o| o.get("mql5Path"))
        .and_then(|v| v.as_str())
        .map(PathBuf::from);
    if let Some(root) = configured
        .or_else(|| workspace.as_deref().and_then(mql5_root))
        .or_else(|| {
            default_mql5_roots()
                .into_iter()
                .find(|p| p.join("Include").is_dir())
        })
    {
        index.index_dir(&root.join("Include"));
    }
    if let Some(w) = &workspace {
        index.index_dir(w);
    }

    let config: compiler::Config = init
        .initialization_options
        .as_ref()
        .and_then(|v| v.get("compilerCheck"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(compiler::Config {
            enabled: false,
            script: None,
            workspace_root: None,
        });
    let check = if config.enabled {
        workspace.clone().map(|w| {
            let root = config.workspace_root.clone().unwrap_or(w);
            let (tx, rx) = compiler::start(config, root.clone());
            (tx, rx, root)
        })
    } else {
        None
    };
    let mut docs: HashMap<Url, (String, i32)> = HashMap::new();
    let mut revisions: HashMap<Url, u64> = HashMap::new();
    let mut publications = compiler::Publications::new();
    let mut latest_entry: HashMap<PathBuf, u64> = HashMap::new();
    let mut next_check = 0u64;
    let mut source_revision = 0u64;
    loop {
        if let Some((_, rx, _)) = &check {
            while let Ok(done) = rx.try_recv() {
                let current = compiler::is_current(&done, &docs, &revisions)
                    && done.save.source_revision == source_revision
                    && latest_entry.get(&done.save.entry) == Some(&done.save.job);
                if !current {
                    continue;
                }
                match done.result {
                    Ok(result) => {
                        if let Some(updates) = publications.apply(done.save.entry, result) {
                            for update in updates {
                                conn.sender.send(Message::Notification(Notification::new(
                                    PublishDiagnostics::METHOD.to_string(),
                                    update,
                                )))?;
                            }
                        }
                    }
                    Err(e) => eprintln!("mql-lsp: {e}"),
                }
            }
        }
        let msg = match conn
            .receiver
            .recv_timeout(std::time::Duration::from_millis(40))
        {
            Ok(msg) => msg,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        };
        match msg {
            Message::Request(req) => {
                if conn.handle_shutdown(&req)? {
                    break;
                }
                let resp = handle_request(req, &docs, &index);
                conn.sender.send(Message::Response(resp))?;
            }
            Message::Notification(n) => {
                if n.method == DidSaveTextDocument::METHOD {
                    if let (Some((tx, _, root)), Ok(p)) = (
                        &check,
                        serde_json::from_value::<DidSaveTextDocumentParams>(n.params),
                    ) {
                        if let Some((_, version)) = docs.get(&p.text_document.uri) {
                            if let Some(path) = path_of(&p.text_document.uri) {
                                if let Some(entry) = compiler::entry_point(&path, root) {
                                    let revision =
                                        revisions.entry(p.text_document.uri.clone()).or_default();
                                    *revision += 1;
                                    let revision = *revision;
                                    next_check += 1;
                                    latest_entry.insert(entry.clone(), next_check);
                                    let _ = tx.send(compiler::Save {
                                        job: next_check,
                                        source_revision,
                                        uri: p.text_document.uri,
                                        version: *version,
                                        revision,
                                        entry,
                                    });
                                }
                            }
                        }
                    }
                } else {
                    let uri = document_uri(&n);
                    handle_notification(n, &mut docs, &mut index);
                    if let Some(uri) = uri {
                        *revisions.entry(uri).or_default() += 1;
                        source_revision += 1;
                    }
                }
            }
            Message::Response(_) => {}
        }
    }
    io.join()?;
    Ok(())
}

fn text_of(docs: &HashMap<Url, (String, i32)>, uri: &Url) -> Option<String> {
    docs.get(uri)
        .map(|(text, _)| text.clone())
        .or_else(|| fs::read_to_string(path_of(uri)?).ok())
}

fn handle_request(req: Request, docs: &HashMap<Url, (String, i32)>, index: &Index) -> Response {
    let id = req.id.clone();
    let ok = |v: serde_json::Value| Response {
        id: id.clone(),
        result: Some(v),
        error: None,
    };
    match req.method.as_str() {
        HoverRequest::METHOD => {
            let p: HoverParams = serde_json::from_value(req.params).unwrap();
            let tdp = p.text_document_position_params;
            let hit = text_of(docs, &tdp.text_document.uri)
                .and_then(|t| word_at(&t, tdp.position))
                .and_then(|w| index.symbols.get(&w));
            ok(match hit {
                Some(v) => serde_json::to_value(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: v
                            .iter()
                            .take(5)
                            .map(markdown)
                            .collect::<Vec<_>>()
                            .join("\n\n---\n\n"),
                    }),
                    range: None,
                })
                .unwrap(),
                None => serde_json::Value::Null,
            })
        }
        GotoDefinition::METHOD => {
            let p: GotoDefinitionParams = serde_json::from_value(req.params).unwrap();
            let tdp = p.text_document_position_params;
            let locs: Vec<Location> = text_of(docs, &tdp.text_document.uri)
                .and_then(|t| word_at(&t, tdp.position))
                .and_then(|w| index.symbols.get(&w))
                .map(|v| {
                    v.iter()
                        .filter_map(|s| {
                            let (path, line) = s.loc.as_ref()?;
                            let pos = Position::new(*line, 0);
                            Some(Location::new(
                                Url::from_file_path(path).ok()?,
                                Range::new(pos, pos),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            ok(serde_json::to_value(locs).unwrap())
        }
        Completion::METHOD => {
            let items: Vec<CompletionItem> = index
                .symbols
                .values()
                .filter_map(|v| v.first())
                .map(|s| CompletionItem {
                    label: s.name.clone(),
                    kind: Some(completion_kind(s.kind)),
                    detail: Some(s.signature.clone()),
                    documentation: (!s.doc.is_empty()).then(|| {
                        Documentation::MarkupContent(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: s.doc.clone(),
                        })
                    }),
                    ..Default::default()
                })
                .collect();
            ok(serde_json::to_value(CompletionResponse::Array(items)).unwrap())
        }
        _ => Response {
            id,
            result: Some(serde_json::Value::Null),
            error: None,
        },
    }
}

fn document_uri(n: &Notification) -> Option<Url> {
    match n.method.as_str() {
        DidOpenTextDocument::METHOD
        | DidChangeTextDocument::METHOD
        | DidCloseTextDocument::METHOD => n
            .params
            .get("textDocument")?
            .get("uri")
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
        _ => None,
    }
}

fn handle_notification(n: Notification, docs: &mut HashMap<Url, (String, i32)>, index: &mut Index) {
    match n.method.as_str() {
        DidOpenTextDocument::METHOD => {
            if let Ok(p) = serde_json::from_value::<DidOpenTextDocumentParams>(n.params) {
                if let Some(path) = path_of(&p.text_document.uri) {
                    index.index_text(&path, &p.text_document.text);
                }
                docs.insert(
                    p.text_document.uri,
                    (p.text_document.text, p.text_document.version),
                );
            }
        }
        DidChangeTextDocument::METHOD => {
            if let Ok(p) = serde_json::from_value::<DidChangeTextDocumentParams>(n.params) {
                if let Some(c) = p.content_changes.into_iter().last() {
                    if let Some(path) = path_of(&p.text_document.uri) {
                        index.index_text(&path, &c.text);
                    }
                    docs.insert(p.text_document.uri, (c.text, p.text_document.version));
                }
            }
        }
        DidCloseTextDocument::METHOD => {
            if let Ok(p) = serde_json::from_value::<DidCloseTextDocumentParams>(n.params) {
                docs.remove(&p.text_document.uri);
            }
        }
        _ => {}
    }
}
