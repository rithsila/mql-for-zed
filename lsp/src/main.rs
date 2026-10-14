mod compiler;
mod explainer;
mod hover;
mod hover_config;
mod index;
mod lint;
mod localization;
mod set_file;
mod snippets;

use hover_config::HoverConfig;
use index::{Index, Kind, NOT_TYPES};
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

fn word_range_at(text: &str, pos: Position) -> Option<(String, Range)> {
    let line = text.lines().nth(pos.line as usize)?;
    let mut char_indices = Vec::new();
    let mut current_utf16 = 0u32;
    for c in line.chars() {
        let len16 = c.len_utf16() as u32;
        char_indices.push((c, current_utf16, current_utf16 + len16));
        current_utf16 += len16;
    }

    if char_indices.is_empty() {
        return None;
    }

    let target = pos.character;
    let cursor_idx = if target >= current_utf16 {
        char_indices.len()
    } else {
        char_indices
            .iter()
            .position(|(_, start, end)| target >= *start && target < *end)
            .unwrap_or(char_indices.len())
    };

    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut s = cursor_idx;
    let mut e = cursor_idx;

    while s > 0 && is_ident(char_indices[s - 1].0) {
        s -= 1;
    }
    while e < char_indices.len() && is_ident(char_indices[e].0) {
        e += 1;
    }

    if s < e {
        let word: String = char_indices[s..e].iter().map(|(c, _, _)| *c).collect();
        let start_col = char_indices[s].1;
        let end_col = char_indices[e - 1].2;
        let range = Range::new(
            Position::new(pos.line, start_col),
            Position::new(pos.line, end_col),
        );
        Some((word, range))
    } else {
        None
    }
}

fn word_at(text: &str, pos: Position) -> Option<String> {
    word_range_at(text, pos).map(|(w, _)| w)
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

fn path_of(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
}

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "explain" => {
                let snippet = if args.len() > 2 {
                    args[2..].join(" ")
                } else {
                    use std::io::Read;
                    let mut buffer = String::new();
                    let _ = std::io::stdin().read_to_string(&mut buffer);
                    buffer
                };
                let explanation = explainer::explain_code(&snippet);
                println!("{}", explanation);

                // Auto-register extracted structs/classes/inputs/functions into .mql/knowledge.json
                let knowledge = explainer::generate_file_knowledge(&snippet);
                if !knowledge.is_empty() {
                    let _ = explainer::write_workspace_knowledge(Path::new("."), knowledge);
                }
                return Ok(());
            }
            "generate-knowledge" => {
                if args.len() < 3 {
                    eprintln!("Usage: mql-lsp generate-knowledge <file.mq5|file.mqh>");
                    std::process::exit(1);
                }
                let target_path = Path::new(&args[2]);
                if !target_path.exists() {
                    eprintln!("Error: file '{}' not found.", target_path.display());
                    std::process::exit(1);
                }
                let content = fs::read_to_string(target_path)?;
                let knowledge = explainer::generate_file_knowledge(&content);
                let workspace = target_path.parent().unwrap_or_else(|| Path::new("."));
                let count = explainer::write_workspace_knowledge(workspace, knowledge)?;
                println!(
                    "✅ Generated {} knowledge entries in {}/.mql/knowledge.json",
                    count,
                    workspace.display()
                );
                return Ok(());
            }
            "--help" | "-h" => {
                println!("mql-lsp: MetaTrader 5 Language Server & AI Vibe Explainer");
                println!();
                println!("Usage:");
                println!("  mql-lsp                         Start LSP server on stdio (for Zed)");
                println!(
                    "  mql-lsp explain \"<code snippet>\"  Explain code in plain English & Khmer"
                );
                println!(
                    "  mql-lsp generate-knowledge <file> Generate .mql/knowledge.json hover docs"
                );
                println!("  mql-lsp --version               Show version");
                return Ok(());
            }
            "--version" | "-v" => {
                println!("mql-lsp {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => {}
        }
    }

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
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
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
        index.set_workspace(w.clone());
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

    let lint_config: lint::Config = init
        .initialization_options
        .as_ref()
        .and_then(|v| v.get("lint"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let mut hover_config =
        hover_config::from_initialization_options(init.initialization_options.as_ref());

    let mut docs: HashMap<Url, (String, i32)> = HashMap::new();
    let mut revisions: HashMap<Url, u64> = HashMap::new();
    let mut publications = compiler::Publications::new();
    let mut lints_by_uri: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
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
                            for mut update in updates {
                                if let Some(lints) = lints_by_uri.get(&update.uri) {
                                    update.diagnostics.extend(lints.iter().cloned());
                                }
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
                let resp = handle_request(req, &docs, &index, &hover_config);
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
                    handle_notification(n, &mut docs, &mut index, &mut hover_config);
                    if let Some(uri) = uri {
                        if let Some((text, version)) = docs.get(&uri) {
                            let mut lints = lint::check(text, &uri, &lint_config);
                            if let Some(path) = path_of(&uri) {
                                if path.extension().is_some_and(|e| e == "set") {
                                    lints.extend(set_file::check(text, &path));
                                }
                            }
                            let prev_lints = lints_by_uri.get(&uri);
                            let changed = match prev_lints {
                                Some(prev) => prev != &lints,
                                None => !lints.is_empty(),
                            };
                            if changed {
                                lints_by_uri.insert(uri.clone(), lints.clone());
                                let mut combined = lints;
                                combined.extend(publications.get(&uri));
                                let _ = conn.sender.send(Message::Notification(Notification::new(
                                    PublishDiagnostics::METHOD.to_string(),
                                    PublishDiagnosticsParams {
                                        uri: uri.clone(),
                                        diagnostics: combined,
                                        version: Some(*version),
                                    },
                                )));
                            }
                        } else {
                            lints_by_uri.remove(&uri);
                        }

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

fn handle_request(
    req: Request,
    docs: &HashMap<Url, (String, i32)>,
    index: &Index,
    hover_config: &HoverConfig,
) -> Response {
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

            if !hover_config.enabled {
                return ok(serde_json::Value::Null);
            }

            if let Some(path) = path_of(&tdp.text_document.uri) {
                if path.extension().is_some_and(|e| e == "set") {
                    if let Some(text) = text_of(docs, &tdp.text_document.uri) {
                        if let Some(line) = text.lines().nth(tdp.position.line as usize) {
                            if let Some(eq_idx) = line.find('=') {
                                let key = line[..eq_idx].trim();
                                let key_start = line
                                    .chars()
                                    .take_while(|c| c.is_whitespace())
                                    .map(|c| c.len_utf16() as u32)
                                    .sum::<u32>();
                                let key_len =
                                    key.chars().map(|c| c.len_utf16() as u32).sum::<u32>();
                                let key_end = key_start + key_len;
                                let eq_utf16 = line[..=eq_idx]
                                    .chars()
                                    .map(|c| c.len_utf16() as u32)
                                    .sum::<u32>();
                                if tdp.position.character <= eq_utf16 {
                                    if let Some(inputs) =
                                        set_file::get_mq5_inputs_for_set_file(&path)
                                    {
                                        if let Some(param) = inputs.iter().find(|i| i.name == key) {
                                            let mq5_path = path.with_extension("mq5");
                                            if let Some(hover_content) =
                                                hover::render_set_input_hover(
                                                    param,
                                                    &mq5_path,
                                                    hover_config,
                                                )
                                            {
                                                let range = Range::new(
                                                    Position::new(tdp.position.line, key_start),
                                                    Position::new(tdp.position.line, key_end),
                                                );
                                                return ok(serde_json::to_value(Hover {
                                                    contents: HoverContents::Markup(
                                                        MarkupContent {
                                                            kind: MarkupKind::Markdown,
                                                            value: hover_content,
                                                        },
                                                    ),
                                                    range: Some(range),
                                                })
                                                .unwrap());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    return ok(serde_json::Value::Null);
                }
            }

            let word_and_range =
                text_of(docs, &tdp.text_document.uri).and_then(|t| word_range_at(&t, tdp.position));

            if let Some((word, range)) = word_and_range {
                // If not a language keyword, check symbol index
                if !NOT_TYPES.contains(&word.as_str()) && word != "input" && word != "sinput" {
                    if let Some(symbols) = index.symbols_for_hover(&word) {
                        if let Some(hover_content) =
                            hover::render_symbol_hover(&symbols, hover_config)
                        {
                            return ok(serde_json::to_value(Hover {
                                contents: HoverContents::Markup(MarkupContent {
                                    kind: MarkupKind::Markdown,
                                    value: hover_content,
                                }),
                                range: Some(range),
                            })
                            .unwrap());
                        }
                    }
                }
            }

            // Fallback: If no symbol hover was rendered (e.g. cursor is on keyword, operator,
            // condition, or statement), inspect statement at position for Context-Aware Vibe Explainer!
            if let Some(doc_text) = text_of(docs, &tdp.text_document.uri) {
                if let Some((explainer_content, explainer_range)) =
                    hover::render_statement_hover(&doc_text, tdp.position, hover_config)
                {
                    return ok(serde_json::to_value(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: explainer_content,
                        }),
                        range: Some(explainer_range),
                    })
                    .unwrap());
                }
            }

            ok(serde_json::Value::Null)
        }
        GotoDefinition::METHOD => {
            let p: GotoDefinitionParams = serde_json::from_value(req.params).unwrap();
            let tdp = p.text_document_position_params;

            if let Some(path) = path_of(&tdp.text_document.uri) {
                if path.extension().is_some_and(|e| e == "set") {
                    if let Some(text) = text_of(docs, &tdp.text_document.uri) {
                        if let Some(line) = text.lines().nth(tdp.position.line as usize) {
                            if let Some(eq_idx) = line.find('=') {
                                let key = line[..eq_idx].trim();
                                if (tdp.position.character as usize) <= eq_idx {
                                    if let Some(inputs) =
                                        set_file::get_mq5_inputs_for_set_file(&path)
                                    {
                                        if let Some(param) = inputs.iter().find(|i| i.name == key) {
                                            let mq5_path = path.with_extension("mq5");
                                            if let Ok(uri) = Url::from_file_path(mq5_path) {
                                                let pos =
                                                    Position::new(param.line, param.character);
                                                return ok(serde_json::to_value(vec![
                                                    Location::new(uri, Range::new(pos, pos)),
                                                ])
                                                .unwrap());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

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
            let p: CompletionParams = serde_json::from_value(req.params).unwrap();
            let mut items: Vec<CompletionItem> = vec![];

            if let Some(path) = path_of(&p.text_document_position.text_document.uri) {
                if path.extension().is_some_and(|e| e == "set") {
                    if let Some(inputs) = set_file::get_mq5_inputs_for_set_file(&path) {
                        items.extend(inputs.into_iter().map(|inp| CompletionItem {
                            label: inp.name.clone(),
                            kind: Some(CompletionItemKind::PROPERTY),
                            detail: Some(format!(
                                "{} (default: {})",
                                inp.type_name, inp.default_value
                            )),
                            insert_text: Some(format!("{}=", inp.name)),
                            ..Default::default()
                        }));
                    }
                }
            }

            if items.is_empty() {
                items.extend(snippets::all_completion_items());
                let symbols: Vec<CompletionItem> = index
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
                items.extend(symbols);
            }
            ok(serde_json::to_value(CompletionResponse::Array(items)).unwrap())
        }
        CodeActionRequest::METHOD => {
            let p: CodeActionParams = serde_json::from_value(req.params).unwrap();
            let mut actions: Vec<CodeActionOrCommand> = Vec::new();

            if let Some(doc_text) = text_of(docs, &p.text_document.uri) {
                let range = p.range;
                let start_line = range.start.line as usize;
                let end_line = range.end.line as usize;
                let lines: Vec<&str> = doc_text.lines().collect();

                let selected_text = if start_line == end_line {
                    lines.get(start_line).map(|l| {
                        let start = (range.start.character as usize).min(l.len());
                        let end = (range.end.character as usize).min(l.len());
                        if start < end {
                            l[start..end].to_string()
                        } else {
                            l.trim().to_string()
                        }
                    })
                } else if start_line < end_line && end_line < lines.len() {
                    let mut s = String::new();
                    for l in &lines[start_line..=end_line] {
                        s.push_str(l);
                        s.push('\n');
                    }
                    Some(s.trim().to_string())
                } else {
                    None
                };

                if let Some(snippet) = selected_text {
                    if !snippet.is_empty() {
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: "🤖 Explain Selection (MQL Vibe Explainer)".to_string(),
                            kind: Some(CodeActionKind::REFACTOR),
                            command: Some(Command {
                                title: "Explain Selection".to_string(),
                                command: "mql.explain".to_string(),
                                arguments: Some(vec![serde_json::to_value(&snippet).unwrap()]),
                            }),
                            ..Default::default()
                        }));
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: "💡 Generate Workspace Knowledge for This File".to_string(),
                            kind: Some(CodeActionKind::REFACTOR),
                            command: Some(Command {
                                title: "Generate Knowledge".to_string(),
                                command: "mql.generateKnowledge".to_string(),
                                arguments: Some(vec![serde_json::to_value(
                                    p.text_document.uri.to_string(),
                                )
                                .unwrap()]),
                            }),
                            ..Default::default()
                        }));
                    }
                }
            }

            ok(serde_json::to_value(actions).unwrap())
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

fn handle_notification(
    n: Notification,
    docs: &mut HashMap<Url, (String, i32)>,
    index: &mut Index,
    hover_config: &mut HoverConfig,
) {
    match n.method.as_str() {
        DidChangeConfiguration::METHOD => {
            if let Ok(p) = serde_json::from_value::<DidChangeConfigurationParams>(n.params) {
                if p.settings.get("hover").is_some() {
                    *hover_config = hover_config::from_initialization_options(Some(&p.settings));
                } else if let Some(mql) = p.settings.get("mql-lsp") {
                    if let Some(init_opts) = mql.get("initialization_options") {
                        *hover_config = hover_config::from_initialization_options(Some(init_opts));
                    }
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hover_config::LanguageConfig;
    use crate::hover_config::PrimaryLanguage;
    use lsp_types::request::HoverRequest;
    use serde_json::json;

    #[test]
    fn test_word_range_at_ascii() {
        let text = "int   OrderSend(MqlTradeRequest &request);";
        // Cursor at col 6 ('O')
        let (word, range) = word_range_at(text, Position::new(0, 6)).unwrap();
        assert_eq!(word, "OrderSend");
        assert_eq!(range.start, Position::new(0, 6));
        assert_eq!(range.end, Position::new(0, 15));

        // Cursor at col 10 ('S' inside OrderSend)
        let (word2, range2) = word_range_at(text, Position::new(0, 10)).unwrap();
        assert_eq!(word2, "OrderSend");
        assert_eq!(range2, range);

        // Cursor at col 15 (right after OrderSend, at '(')
        let (word3, range3) = word_range_at(text, Position::new(0, 15)).unwrap();
        assert_eq!(word3, "OrderSend");
        assert_eq!(range3, range);

        // Cursor at col 3 (right after "int", returns "int")
        let (word_int, range_int) = word_range_at(text, Position::new(0, 3)).unwrap();
        assert_eq!(word_int, "int");
        assert_eq!(range_int.start, Position::new(0, 0));
        assert_eq!(range_int.end, Position::new(0, 3));

        // Cursor at col 4 (pure whitespace, not adjacent to any identifier)
        assert!(word_range_at(text, Position::new(0, 4)).is_none());
    }

    #[test]
    fn test_word_range_at_utf16_multibyte_and_surrogates() {
        // Line has:
        // "// " (3 code units)
        // "ស្វាគមន៍" (8 Khmer chars = 8 UTF-16 code units)
        // " " (1 code unit)
        // "🌍" (1 char U+1F30D = 2 UTF-16 code units, surrogate pair)
        // " " (1 code unit)
        // Total prefix = 3 + 8 + 1 + 2 + 1 = 15 UTF-16 code units.
        // Then "OrderSend" (9 code units) -> 15..24.
        let text = "// ស្វាគមន៍ 🌍 OrderSend();";

        let (word, range) = word_range_at(text, Position::new(0, 16)).unwrap();
        assert_eq!(word, "OrderSend");
        assert_eq!(range.start, Position::new(0, 15));
        assert_eq!(range.end, Position::new(0, 24));
    }

    #[test]
    fn test_handle_request_hover_bilingual() {
        let index = Index::new();
        let uri = Url::parse("file:///test/main.mq5").unwrap();
        let mut docs = HashMap::new();
        docs.insert(
            uri.clone(),
            ("void OnTick() { OrderSend(req, res); }".to_string(), 1),
        );

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

        // Col 18 is inside OrderSend
        let req = Request {
            id: 1.into(),
            method: HoverRequest::METHOD.to_string(),
            params: json!(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: Position::new(0, 18),
                },
                work_done_progress_params: Default::default(),
            }),
        };

        let resp = handle_request(req, &docs, &index, &config);
        assert!(resp.error.is_none());
        let hover: Hover = serde_json::from_value(resp.result.unwrap()).unwrap();

        let range = hover.range.unwrap();
        assert_eq!(range.start, Position::new(0, 16));
        assert_eq!(range.end, Position::new(0, 25));

        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("**OrderSend** · Function"));
            assert!(m
                .value
                .contains("**English**\n\nSends a live trade (Buy/Sell)"));
            assert!(m.value.contains("**ខ្មែរ**\n\nផ្ញើសំណើប្រតិបត្តិការ"));
            assert!(m.value.contains("Documentation: bundled reference"));
        } else {
            panic!("Expected HoverContents::Markup");
        }
    }

    #[test]
    fn test_handle_request_hover_khmer_only() {
        let index = Index::new();
        let uri = Url::parse("file:///test/main.mq5").unwrap();
        let mut docs = HashMap::new();
        docs.insert(
            uri.clone(),
            ("void OnTick() { OrderSend(req, res); }".to_string(), 1),
        );

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

        let req = Request {
            id: 2.into(),
            method: HoverRequest::METHOD.to_string(),
            params: json!(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: Position::new(0, 18),
                },
                work_done_progress_params: Default::default(),
            }),
        };

        let resp = handle_request(req, &docs, &index, &config);
        let hover: Hover = serde_json::from_value(resp.result.unwrap()).unwrap();

        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("**OrderSend** · អនុគមន៍ (Function)"));
            assert!(m.value.contains("ផ្ញើសំណើប្រតិបត្តិការ"));
            assert!(!m.value.contains("Sends a live trade")); // No English leak
            assert!(!m.value.contains("**English**"));
        } else {
            panic!("Expected HoverContents::Markup");
        }
    }

    #[test]
    fn test_handle_request_hover_disabled() {
        let index = Index::new();
        let uri = Url::parse("file:///test/main.mq5").unwrap();
        let mut docs = HashMap::new();
        docs.insert(
            uri.clone(),
            ("void OnTick() { OrderSend(req, res); }".to_string(), 1),
        );

        let config = HoverConfig {
            enabled: false,
            ..Default::default()
        };

        let req = Request {
            id: 3.into(),
            method: HoverRequest::METHOD.to_string(),
            params: json!(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: Position::new(0, 18),
                },
                work_done_progress_params: Default::default(),
            }),
        };

        let resp = handle_request(req, &docs, &index, &config);
        assert_eq!(resp.result.unwrap(), serde_json::Value::Null);
    }

    #[test]
    fn test_handle_request_hover_with_knowledge_json() {
        let temp_dir = std::env::temp_dir().join(format!("mql_test_ws_{}", std::process::id()));
        let _ = fs::create_dir_all(temp_dir.join(".mql"));
        let knowledge_file = temp_dir.join(".mql/knowledge.json");
        let knowledge_content = r#"{
            "SymbolContext": {
                "en": "Holds currency trading state.",
                "km": "ផ្ទុកស្ថានភាពជួញដូររូបិយប័ណ្ណ។",
                "vibe": "Per-symbol trading context."
            }
        }"#;
        fs::write(&knowledge_file, knowledge_content).unwrap();

        let mut index = Index::new();
        index.set_workspace(temp_dir.clone());

        let file_path = temp_dir.join("waka.mq5");
        let code = "class SymbolContext\n{\n};\nvoid OnTick() { SymbolContext sc; }";
        fs::write(&file_path, code).unwrap();
        index.index_file(&file_path);

        let uri = Url::from_file_path(&file_path).unwrap();
        let mut docs = HashMap::new();
        docs.insert(uri.clone(), (code.to_string(), 1));

        let mut config = HoverConfig::default();
        config.languages.khmer = true;
        let req = Request {
            id: 4.into(),
            method: HoverRequest::METHOD.to_string(),
            params: json!(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: Position::new(0, 8), // on "SymbolContext"
                },
                work_done_progress_params: Default::default(),
            }),
        };

        let resp = handle_request(req, &docs, &index, &config);
        assert!(resp.error.is_none());
        let hover: Hover = serde_json::from_value(resp.result.unwrap()).unwrap();

        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("**SymbolContext** · Class"));
            assert!(m.value.contains("Holds currency trading state."));
            assert!(m.value.contains("ផ្ទុកស្ថានភាពជួញដូររូបិយប័ណ្ណ។"));
            assert!(m.value.contains("Per-symbol trading context."));
            assert!(m
                .value
                .contains("Documentation: AI Workspace Knowledge (.mql/knowledge.json)"));
        } else {
            panic!("Expected HoverContents::Markup");
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_handle_request_hover_condition_gate() {
        let index = Index::new();
        let uri = Url::parse("file:///test/main.mq5").unwrap();
        let mut docs = HashMap::new();
        let code = "void OnTick()\n{\n   if(AllowHedging && mm != ACCOUNT_MARGIN_MODE_RETAIL_HEDGING)\n   {\n   }\n}";
        docs.insert(uri.clone(), (code.to_string(), 1));

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

        // Hover directly on 'if' keyword at line 2 col 4
        let req = Request {
            id: 5.into(),
            method: HoverRequest::METHOD.to_string(),
            params: json!(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position: Position::new(2, 4),
                },
                work_done_progress_params: Default::default(),
            }),
        };

        let resp = handle_request(req, &docs, &index, &config);
        assert!(resp.error.is_none());
        let hover: Hover = serde_json::from_value(resp.result.unwrap()).unwrap();

        if let HoverContents::Markup(m) = hover.contents {
            assert!(m.value.contains("Condition Gate"));
            assert!(m.value.contains("AllowHedging"));
            assert!(m.value.contains("ACCOUNT_MARGIN_MODE_RETAIL_HEDGING"));
            assert!(m.value.contains("Trader Gotcha"));
        } else {
            panic!("Expected HoverContents::Markup");
        }

        // Also test hovering on operator '&&' at line 2 col 19
        let req_op = Request {
            id: 6.into(),
            method: HoverRequest::METHOD.to_string(),
            params: json!(HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: Position::new(2, 19),
                },
                work_done_progress_params: Default::default(),
            }),
        };

        let resp_op = handle_request(req_op, &docs, &index, &config);
        assert!(resp_op.error.is_none());
        let hover_op: Hover = serde_json::from_value(resp_op.result.unwrap()).unwrap();
        if let HoverContents::Markup(m) = hover_op.contents {
            assert!(m.value.contains("Condition Gate"));
            assert!(m.value.contains("&&"));
        } else {
            panic!("Expected HoverContents::Markup");
        }
    }

    #[test]
    fn test_handle_request_code_action_selection() {
        let index = Index::new();
        let uri = Url::parse("file:///test/main.mq5").unwrap();
        let mut docs = HashMap::new();
        let code = "if(AllowHedging && mm != ACCOUNT_MARGIN_MODE_RETAIL_HEDGING)";
        docs.insert(uri.clone(), (code.to_string(), 1));

        let config = HoverConfig::default();

        let req = Request {
            id: 7.into(),
            method: CodeActionRequest::METHOD.to_string(),
            params: json!(CodeActionParams {
                text_document: TextDocumentIdentifier { uri },
                range: Range::new(Position::new(0, 0), Position::new(0, 60)),
                context: CodeActionContext::default(),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            }),
        };

        let resp = handle_request(req, &docs, &index, &config);
        assert!(resp.error.is_none());
        let actions: Vec<CodeActionOrCommand> =
            serde_json::from_value(resp.result.unwrap()).unwrap();

        assert!(!actions.is_empty());
        let has_explain = actions.iter().any(|a| match a {
            CodeActionOrCommand::CodeAction(ca) => ca.title.contains("Explain Selection"),
            _ => false,
        });
        assert!(has_explain);
    }
}
