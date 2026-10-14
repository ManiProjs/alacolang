//! A small, dependency-light LSP server built directly on Alaco's lexer and parser.
//! The transport is JSON-RPC over stdio; editor-facing work stays synchronous and
//! in-memory so ordinary keystrokes do not spawn compiler subprocesses.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use miette::Diagnostic;
use serde_json::{Value, json};

use crate::{
    ast::{Item, Program, Type},
    compiler::Compiler,
    error::AlacoError,
    lexer::Lexer,
    parser::Parser,
    token::{Token, TokenKind},
};

const TOKEN_TYPES: &[&str] = &[
    "namespace",
    "type",
    "function",
    "parameter",
    "variable",
    "keyword",
    "number",
    "string",
    "operator",
];
const TOKEN_MODIFIERS: &[&str] = &["declaration", "readonly", "defaultLibrary"];

#[derive(Clone, Debug)]
struct Document {
    text: String,
    version: i64,
}

#[derive(Default)]
struct Server {
    documents: HashMap<String, Document>,
    shutdown: bool,
    root_uri: Option<String>,
}

pub fn run() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    // Notifications and request responses share stdout; don't hold a persistent
    // lock here because the notification writer acquires its own short lock.
    let mut writer = stdout;
    let mut server = Server::default();

    while let Some(message) = read_message(&mut reader)? {
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = server.handle(method, &params);

        if let Some(id) = id {
            let response = match result {
                Ok(value) => json!({"jsonrpc":"2.0", "id":id, "result":value}),
                Err((code, message)) => {
                    json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
                }
            };
            write_message(&mut writer, &response)?;
        }
        if server.shutdown && method == "exit" {
            break;
        }
    }
    Ok(())
}

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut content_length = None;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse::<usize>().ok();
            }
        }
    }
    let Some(length) = content_length else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "LSP message has no valid Content-Length",
        ));
    };
    if length > 32 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "LSP message exceeds 32 MiB",
        ));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn write_message(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(value).map_err(io::Error::other)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}

impl Server {
    fn handle(&mut self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        match method {
            "initialize" => {
                self.root_uri = params
                    .get("rootUri")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                Ok(json!({
                    "capabilities": {
                        "positionEncoding":"utf-16",
                        "textDocumentSync":{"openClose":true,"change":2,"save":{"includeText":false}},
                        "completionProvider":{"resolveProvider":true,"triggerCharacters":[".","(",":"]},
                        "hoverProvider":true,
                        "definitionProvider":true,
                        "referencesProvider":true,
                        "documentHighlightProvider":true,
                        "documentSymbolProvider":true,
                        "workspaceSymbolProvider":true,
                        "documentLinkProvider":{"resolveProvider":false},
                        "codeLensProvider":{"resolveProvider":false},
                        "renameProvider":{"prepareProvider":true},
                        "signatureHelpProvider":{"triggerCharacters":["(",","]},
                        "documentFormattingProvider":true,
                        "documentRangeFormattingProvider":true,
                        "documentOnTypeFormattingProvider":{"firstTriggerCharacter":"}","moreTriggerCharacter":["\n"]},
                        "foldingRangeProvider":true,
                        "selectionRangeProvider":true,
                        "semanticTokensProvider":{"legend":{"tokenTypes":TOKEN_TYPES,"tokenModifiers":TOKEN_MODIFIERS},"full":true,"range":true},
                        "codeActionProvider":{"resolveProvider":false,"codeActionKinds":["source.organizeImports"]},
                        "inlayHintProvider":{"resolveProvider":false},
                        "executeCommandProvider":{"commands":["alaco.checkWorkspace","alaco.checkDocument"]},
                        "experimental":{"alacoServerVersion":"0.1.0","incrementalParsing":false}
                    },
                    "serverInfo":{"name":"Alaco Language Server","version":env!("CARGO_PKG_VERSION")}
                }))
            }
            "initialized" | "$ /cancelRequest" | "$/cancelRequest" | "$/setTrace" => {
                Ok(Value::Null)
            }
            "shutdown" => {
                self.shutdown = true;
                Ok(Value::Null)
            }
            "exit" => Ok(Value::Null),
            "textDocument/didOpen" => {
                let doc = params.get("textDocument").unwrap_or(&Value::Null);
                let uri = string(doc, "uri");
                let text = string(doc, "text");
                let version = doc.get("version").and_then(Value::as_i64).unwrap_or(0);
                self.documents
                    .insert(uri.clone(), Document { text, version });
                self.publish_diagnostics(&uri);
                Ok(Value::Null)
            }
            "textDocument/didChange" => {
                let doc = params.get("textDocument").unwrap_or(&Value::Null);
                let uri = string(doc, "uri");
                if let Some(current) = self.documents.get_mut(&uri) {
                    for change in params
                        .get("contentChanges")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        let replacement = string(change, "text");
                        if let Some(range) = change.get("range") {
                            let start = position_offset(
                                &current.text,
                                range.get("start").unwrap_or(&Value::Null),
                            );
                            let end = position_offset(
                                &current.text,
                                range.get("end").unwrap_or(&Value::Null),
                            );
                            let start =
                                clamp_char_boundary(&current.text, start.min(current.text.len()));
                            let end =
                                clamp_char_boundary(&current.text, end.min(current.text.len()))
                                    .max(start);
                            current.text.replace_range(start..end, &replacement);
                        } else {
                            current.text = replacement;
                        }
                    }
                    current.version = doc
                        .get("version")
                        .and_then(Value::as_i64)
                        .unwrap_or(current.version);
                }
                self.publish_diagnostics(&uri);
                Ok(Value::Null)
            }
            "textDocument/didSave" => {
                let uri = string(params.get("textDocument").unwrap_or(&Value::Null), "uri");
                self.publish_diagnostics(&uri);
                Ok(Value::Null)
            }
            "textDocument/didClose" => {
                let uri = string(params.get("textDocument").unwrap_or(&Value::Null), "uri");
                self.documents.remove(&uri);
                self.publish_empty_diagnostics(&uri);
                Ok(Value::Null)
            }
            "textDocument/completion" => Ok(self.completions(params)),
            "completionItem/resolve" => Ok(self.resolve_completion(params)),
            "textDocument/hover" => Ok(self.hover(params)),
            "textDocument/definition"
            | "textDocument/typeDefinition"
            | "textDocument/implementation" => Ok(self.definition(params)),
            "textDocument/references" => Ok(self.references(params)),
            "textDocument/documentHighlight" => Ok(self.highlights(params)),
            "textDocument/documentSymbol" => Ok(self.document_symbols(params)),
            "textDocument/documentLink" => Ok(self.document_links(params)),
            "workspace/symbol" => Ok(self.workspace_symbols(params)),
            "textDocument/rename" => Ok(self.rename(params)),
            "textDocument/prepareRename" => Ok(self.prepare_rename(params)),
            "textDocument/signatureHelp" => Ok(self.signature_help(params)),
            "textDocument/semanticTokens/full" => Ok(self.semantic_tokens(params, None)),
            "textDocument/semanticTokens/range" => {
                Ok(self.semantic_tokens(params, params.get("range")))
            }
            "textDocument/foldingRange" => Ok(self.folding_ranges(params)),
            "textDocument/codeLens" => Ok(self.code_lenses(params)),
            "textDocument/formatting" => Ok(self.formatting(params, None)),
            "textDocument/rangeFormatting" => Ok(self.formatting(params, params.get("range"))),
            "textDocument/onTypeFormatting" => Ok(self.formatting(params, None)),
            "textDocument/codeAction" => Ok(self.code_actions(params)),
            "textDocument/inlayHint" => Ok(self.inlay_hints(params)),
            "textDocument/selectionRange" => Ok(self.selection_ranges(params)),
            "workspace/executeCommand" => {
                if params.get("command").and_then(Value::as_str) == Some("alaco.checkWorkspace") {
                    let mut uris = self.documents.keys().cloned().collect::<Vec<_>>();
                    uris.sort();
                    for uri in uris {
                        self.publish_diagnostics(&uri);
                    }
                } else if params.get("command").and_then(Value::as_str)
                    == Some("alaco.checkDocument")
                {
                    if let Some(uri) = params
                        .get("arguments")
                        .and_then(Value::as_array)
                        .and_then(|args| args.first())
                        .and_then(Value::as_str)
                    {
                        self.publish_diagnostics(uri);
                    }
                }
                Ok(Value::Null)
            }
            "$ /progress" | "$/progress" | "$/logTrace" => Ok(Value::Null),
            _ => Err((-32601, format!("method not supported: {method}"))),
        }
    }

    fn source(&self, params: &Value) -> Option<(&str, &str)> {
        let uri = string(params.get("textDocument").unwrap_or(&Value::Null), "uri");
        self.documents
            .get_key_value(&uri)
            .map(|(key, doc)| (key.as_str(), doc.text.as_str()))
    }

    /// Return workspace sources with unsaved buffers taking precedence over disk.
    /// This small on-demand index makes navigation useful before every file has
    /// been opened in an editor, without introducing a background watcher.
    fn workspace_sources(&self) -> Vec<(String, String)> {
        let mut sources = self
            .documents
            .iter()
            .map(|(uri, doc)| (uri.clone(), doc.text.clone()))
            .collect::<HashMap<_, _>>();

        if let Some(root) = self.root_uri.as_deref().map(uri_to_path) {
            let mut files = Vec::new();
            collect_aco_files(Path::new(&root), &mut files);
            for path in files {
                let uri = path_to_uri(&path);
                if sources.contains_key(&uri) {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&path) {
                    sources.insert(uri, text);
                }
            }
        }

        let mut sources = sources.into_iter().collect::<Vec<_>>();
        sources.sort_by(|left, right| left.0.cmp(&right.0));
        sources
    }

    fn publish_diagnostics(&self, uri: &str) {
        let Some(doc) = self.documents.get(uri) else {
            return;
        };
        let diagnostics = diagnostics_for(&doc.text, uri);
        self.notify(&json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":uri,"version":doc.version,"diagnostics":diagnostics}}));
    }

    fn publish_empty_diagnostics(&self, uri: &str) {
        self.notify(&json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":uri,"diagnostics":[]}}));
    }

    fn notify(&self, message: &Value) {
        let Ok(body) = serde_json::to_vec(message) else {
            return;
        };
        let mut stdout = io::stdout().lock();
        let _ = write!(stdout, "Content-Length: {}\r\n\r\n", body.len());
        let _ = stdout.write_all(&body);
        let _ = stdout.flush();
    }

    fn tokens_for(&self, uri: &str) -> Vec<Token> {
        self.documents
            .get(uri)
            .and_then(|doc| lex(&doc.text).ok())
            .unwrap_or_default()
    }

    fn completions(&self, params: &Value) -> Value {
        let Some((uri, source)) = self.source(params) else {
            return json!([]);
        };
        let position = params.pointer("/position").unwrap_or(&Value::Null);
        let offset = position_offset(source, position);
        let prefix = source[..clamp_char_boundary(source, offset)]
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        let mut entries = BTreeMap::<String, (u32, String)>::new();
        for (label, kind, detail) in [
            ("fn", 14, "Declare a function"),
            ("struct", 14, "Declare a struct"),
            ("import", 14, "Import a module"),
            ("from", 14, "Import selected symbols"),
            ("as", 14, "Import alias"),
            ("let", 14, "Declare an immutable binding"),
            ("mut", 14, "Mutable binding modifier"),
            ("return", 14, "Return from a function"),
            ("if", 14, "Conditional"),
            ("else", 14, "Alternative branch"),
            ("match", 14, "Pattern match"),
            ("loop", 14, "Loop"),
            ("repeat", 14, "Counted loop"),
            ("while", 14, "Conditional loop"),
            ("for", 14, "Iterate over values"),
            ("in", 14, "Loop membership keyword"),
            ("stop", 14, "Exit a loop"),
            ("skip", 14, "Continue a loop"),
            ("true", 12, "Boolean literal"),
            ("false", 12, "Boolean literal"),
            ("and", 14, "Boolean conjunction"),
            ("or", 14, "Boolean disjunction"),
            ("Int", 25, "Built-in integer type"),
            ("Float", 25, "Built-in floating-point type"),
            ("Bool", 25, "Built-in boolean type"),
            ("String", 25, "Built-in string type"),
            ("Void", 25, "No return value"),
            ("println", 3, "println(value) — print a value and newline"),
            ("print", 3, "print(value) — print without newline"),
            ("eprintln", 3, "eprintln(value) — print to stderr"),
            (
                "eprint",
                3,
                "eprint(value) — print to stderr without newline",
            ),
        ] {
            entries.insert(label.into(), (kind, detail.into()));
        }
        for module in [
            "std.io",
            "std.math",
            "std.string",
            "std.stats",
            "std.geometry",
            "std.units",
            "std.validation",
            "std.fs",
            "std.env",
            "std.path",
            "std.json",
            "std.toml",
            "std.http",
            "std.timing",
        ] {
            entries.insert(module.into(), (9, "Alaco standard library module".into()));
        }
        for (label, detail) in [
            ("math.gcd", "Greatest common divisor"),
            ("math.lcm", "Least common multiple"),
            ("math.pow_int", "Integer exponentiation"),
            ("math.is_prime", "Primality test"),
            ("string.concat", "Concatenate strings"),
            ("string.repeat_string", "Repeat a string"),
            ("fs.read_text", "Read a text file"),
            ("fs.write_text", "Write a text file"),
            ("env.get", "Read an environment variable"),
            ("path.join", "Join path components"),
        ] {
            entries.insert(label.into(), (3, detail.into()));
        }
        if let Ok(program) = parse(source) {
            for item in &program.items {
                match item {
                    Item::Function(function) => {
                        let signature = signature(function);
                        entries.insert(function.name.clone(), (3, signature));
                        for param in &function.params {
                            entries.insert(
                                param.name.clone(),
                                (6, format!("parameter: {}", type_name(&param.ty))),
                            );
                        }
                    }
                    Item::Struct(s) => {
                        entries.insert(s.name.clone(), (22, "User-defined struct".into()));
                    }
                    Item::Import(import) => {
                        if let Some(last) = import.path.last() {
                            entries.insert(last.clone(), (9, "Imported module".into()));
                        }
                        for symbol in &import.symbols {
                            entries.insert(symbol.clone(), (3, "Imported function".into()));
                        }
                    }
                    Item::Statement(_) => {}
                }
            }
        }
        if let Ok(tokens) = lex(source) {
            for token in tokens {
                if let TokenKind::Identifier(name) = token.kind {
                    entries
                        .entry(name.clone())
                        .or_insert((6, "Identifier in this document".into()));
                }
            }
        }
        let is_member = source[..clamp_char_boundary(source, offset)]
            .trim_end()
            .ends_with('.');
        let items = entries.into_iter().filter(|(label, _)| label.starts_with(&prefix) || (is_member && label.contains('.'))).map(|(label, (kind, detail))| {
            json!({"label":label,"kind":kind,"detail":detail,"sortText":format!("{:02}-{label}",kind),"insertText":label,"data":{"uri":uri}})
        }).collect::<Vec<_>>();
        json!({"isIncomplete":false,"items":items})
    }

    fn resolve_completion(&self, item: &Value) -> Value {
        let mut resolved = item.clone();
        let label = string(item, "label");
        let detail = string(item, "detail");
        let documentation = keyword_help(&label)
            .or_else(|| standard_help(&label))
            .map(str::to_owned)
            .unwrap_or_else(|| {
                if detail.is_empty() {
                    format!("Alaco symbol `{label}`.")
                } else {
                    format!("{detail}.")
                }
            });
        resolved["documentation"] = json!({"kind":"markdown","value":documentation});
        resolved
    }

    fn code_lenses(&self, params: &Value) -> Value {
        let Some((uri, source)) = self.source(params) else {
            return json!([]);
        };
        let range = json!({
            "start":{"line":0,"character":0},
            "end":{"line":0,"character":0}
        });
        let function_count = parse(source)
            .map(|program| {
                program
                    .items
                    .iter()
                    .filter(|item| matches!(item, Item::Function(_)))
                    .count()
            })
            .unwrap_or(0);
        json!([{
            "range":range,
            "command":{
                "title":format!("Check Alaco file · {function_count} function{}", if function_count == 1 { "" } else { "s" }),
                "command":"alaco.checkDocument",
                "arguments":[uri]
            }
        }])
    }

    fn hover(&self, params: &Value) -> Value {
        let Some((uri, source)) = self.source(params) else {
            return Value::Null;
        };
        let tokens = self.tokens_for(uri);
        let offset = position_offset(source, params.pointer("/position").unwrap_or(&Value::Null));
        let Some(token) = token_at(&tokens, offset) else {
            return Value::Null;
        };
        let name = token.lexeme.trim();
        let mut markdown = keyword_help(name).map(str::to_owned);
        if markdown.is_none() {
            if let Ok(program) = parse(source) {
                for item in program.items {
                    match item {
                        Item::Function(function) if function.name == name => {
                            markdown = Some(format!(
                                "```alaco\n{}\n```\n\nAlaco function.",
                                signature(&function)
                            ))
                        }
                        Item::Struct(s) if s.name == name => {
                            markdown = Some(format!(
                                "```alaco\nstruct {} {{ {} }}\n```",
                                s.name,
                                s.fields
                                    .iter()
                                    .map(|f| format!("{}: {}", f.name, type_name(&f.ty)))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ))
                        }
                        _ => {}
                    }
                }
            }
        }
        if markdown.is_none() {
            markdown = standard_help(name).map(str::to_owned);
        }
        if markdown.is_none() && matches!(token.kind, TokenKind::Integer(_)) {
            markdown = Some("Integer literal (`Int`).".into());
        }
        if markdown.is_none() && matches!(token.kind, TokenKind::Float(_)) {
            markdown = Some("Floating-point literal (`Float`).".into());
        }
        if markdown.is_none()
            && matches!(
                token.kind,
                TokenKind::String(_) | TokenKind::StringTemplate(_)
            )
        {
            markdown = Some("String literal (`String`).".into());
        }
        if markdown.is_none() && matches!(token.kind, TokenKind::True | TokenKind::False) {
            markdown = Some("Boolean literal (`Bool`).".into());
        }
        markdown.map(|value| json!({"contents":{"kind":"markdown","value":value},"range":span_range(source, token.span.offset(), token.span.offset()+token.span.len())})).unwrap_or(Value::Null)
    }

    fn definition(&self, params: &Value) -> Value {
        let Some((uri, source)) = self.source(params) else {
            return Value::Null;
        };
        let offset = position_offset(source, params.pointer("/position").unwrap_or(&Value::Null));
        let tokens = self.tokens_for(uri);
        let Some(token) = token_at(&tokens, offset) else {
            return Value::Null;
        };
        let name = token.lexeme.trim();
        for (candidate_uri, candidate_source) in self.workspace_sources() {
            if let Ok(tokens) = lex(&candidate_source) {
                for (index, candidate) in tokens.iter().enumerate() {
                    if candidate.lexeme == name && is_declaration(&tokens, index) {
                        return json!({"uri":candidate_uri,"range":span_range(&candidate_source,candidate.span.offset(),candidate.span.offset()+candidate.span.len())});
                    }
                }
            }
        }
        Value::Null
    }

    fn references(&self, params: &Value) -> Value {
        let Some((uri, source)) = self.source(params) else {
            return json!([]);
        };
        let offset = position_offset(source, params.pointer("/position").unwrap_or(&Value::Null));
        let tokens = self.tokens_for(uri);
        let Some(token) = token_at(&tokens, offset) else {
            return json!([]);
        };
        let name = token.lexeme.trim();
        let include_decl = params
            .pointer("/context/includeDeclaration")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let mut locations = Vec::new();
        for (doc_uri, doc_source) in self.workspace_sources() {
            if let Ok(tokens) = lex(&doc_source) {
                for (index, candidate) in tokens.iter().enumerate() {
                    if candidate.lexeme == name && (include_decl || !is_declaration(&tokens, index))
                    {
                        locations.push(json!({"uri":doc_uri,"range":span_range(&doc_source,candidate.span.offset(),candidate.span.offset()+candidate.span.len())}));
                    }
                }
            }
        }
        json!(locations)
    }

    fn highlights(&self, params: &Value) -> Value {
        self.references(params)
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|location| json!({"range":location["range"],"kind":1}))
            .collect::<Vec<_>>()
            .into()
    }

    fn document_symbols(&self, params: &Value) -> Value {
        let Some((_, source)) = self.source(params) else {
            return json!([]);
        };
        let Ok(program) = parse(source) else {
            return json!([]);
        };
        symbols_for(&program, source)
    }

    fn document_links(&self, params: &Value) -> Value {
        let Some((uri, source)) = self.source(params) else {
            return json!([]);
        };
        let document_path = uri_to_path(uri);
        let root = self
            .root_uri
            .as_deref()
            .map(uri_to_path)
            .map(PathBuf::from)
            .or_else(|| Path::new(&document_path).parent().map(Path::to_path_buf));
        let Some(root) = root else {
            return json!([]);
        };

        let mut links = Vec::new();
        let mut offset = 0usize;
        for line in source.split_inclusive('\n') {
            let trimmed = line.trim_start();
            let leading = line.len() - trimmed.len();
            let module_start_in_trimmed = if let Some(rest) = trimmed.strip_prefix("import ") {
                Some(trimmed.len() - rest.len())
            } else if let Some(rest) = trimmed.strip_prefix("from ") {
                Some(trimmed.len() - rest.len())
            } else {
                None
            };
            if let Some(module_start) = module_start_in_trimmed {
                let rest = &trimmed[module_start..];
                let module = rest
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.')
                    .collect::<String>();
                if !module.is_empty() {
                    let relative = module.replace('.', "/");
                    let candidates = [
                        root.join(format!("{relative}.aco")),
                        root.join("src").join(format!("{relative}.aco")),
                        root.join("std").join(format!("{relative}.aco")),
                    ];
                    if let Some(target) = candidates.iter().find(|candidate| candidate.is_file()) {
                        let start = offset + leading + module_start;
                        let end = start + module.len();
                        links.push(json!({
                            "range":span_range(source, start, end),
                            "target":path_to_uri(target)
                        }));
                    }
                }
            }
            offset += line.len();
        }
        json!(links)
    }

    fn workspace_symbols(&self, params: &Value) -> Value {
        let query = string(params, "query").to_lowercase();
        let mut symbols = Vec::new();
        for (uri, source) in self.workspace_sources() {
            if let Ok(program) = parse(&source) {
                for item in &program.items {
                    let (name, kind, start, end) = match item {
                        Item::Function(f) => (&f.name, 12, f.span.start, f.span.end),
                        Item::Struct(s) => (&s.name, 23, s.span.start, s.span.end),
                        _ => continue,
                    };
                    if name.to_lowercase().contains(&query) {
                        symbols.push(json!({"name":name,"kind":kind,"location":{"uri":uri,"range":span_range(&source,start,end)}}));
                    }
                }
            }
        }
        json!(symbols)
    }

    fn prepare_rename(&self, params: &Value) -> Value {
        let Some((_, source)) = self.source(params) else {
            return Value::Null;
        };
        let offset = position_offset(source, params.pointer("/position").unwrap_or(&Value::Null));
        token_at(&self.tokens_for(&string(params.get("textDocument").unwrap_or(&Value::Null), "uri")), offset)
            .filter(|token| matches!(token.kind, TokenKind::Identifier(_)))
            .map(|token| json!({"range":span_range(source,token.span.offset(),token.span.offset()+token.span.len()),"placeholder":token.lexeme}))
            .unwrap_or(Value::Null)
    }

    fn rename(&self, params: &Value) -> Value {
        let Some((uri, source)) = self.source(params) else {
            return Value::Null;
        };
        let offset = position_offset(source, params.pointer("/position").unwrap_or(&Value::Null));
        let tokens = self.tokens_for(uri);
        let Some(token) = token_at(&tokens, offset) else {
            return Value::Null;
        };
        if !matches!(token.kind, TokenKind::Identifier(_)) {
            return Value::Null;
        }
        let old = token.lexeme.trim();
        let new_name = string(params, "newName");
        if new_name.is_empty()
            || !new_name
                .chars()
                .enumerate()
                .all(|(i, c)| c == '_' || c.is_alphanumeric() && (i > 0 || !c.is_numeric()))
        {
            return Value::Null;
        }
        let mut changes = serde_json::Map::new();
        for (doc_uri, doc_source) in self.workspace_sources() {
            if let Ok(tokens) = lex(&doc_source) {
                let edits = tokens.iter().filter(|t| t.lexeme == old && matches!(t.kind, TokenKind::Identifier(_))).map(|t| json!({"range":span_range(&doc_source,t.span.offset(),t.span.offset()+t.span.len()),"newText":new_name})).collect::<Vec<_>>();
                if !edits.is_empty() {
                    changes.insert(doc_uri.clone(), json!(edits));
                }
            }
        }
        json!({"changes":changes})
    }

    fn signature_help(&self, params: &Value) -> Value {
        let Some((uri, source)) = self.source(params) else {
            return Value::Null;
        };
        let offset = position_offset(source, params.pointer("/position").unwrap_or(&Value::Null));
        let prefix = &source[..clamp_char_boundary(source, offset)];
        let Some(open) = prefix.rfind('(') else {
            return Value::Null;
        };
        let name = prefix[..open]
            .trim_end()
            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
            .next_back()
            .unwrap_or("");
        let active_parameter = prefix[open + 1..].chars().filter(|c| *c == ',').count() as u64;
        let Ok(program) = parse(source) else {
            return Value::Null;
        };
        for item in program.items {
            if let Item::Function(function) = item {
                if function.name == name || format!("{}.{}", "", function.name) == name {
                    let label = signature(&function);
                    let parameters = function
                        .params
                        .iter()
                        .map(|p| json!({"label":format!("{}: {}",p.name,type_name(&p.ty))}))
                        .collect::<Vec<_>>();
                    return json!({"signatures":[{"label":label,"parameters":parameters}],"activeSignature":0,"activeParameter":active_parameter.min(function.params.len().saturating_sub(1) as u64)});
                }
            }
        }
        let _ = uri;
        Value::Null
    }

    fn semantic_tokens(&self, params: &Value, range: Option<&Value>) -> Value {
        let Some((uri, source)) = self.source(params) else {
            return json!({"data":[]});
        };
        let tokens = self.tokens_for(uri);
        let mut function_names = HashSet::new();
        let mut type_names = HashSet::new();
        if let Ok(program) = parse(source) {
            for item in program.items {
                match item {
                    Item::Function(function) => {
                        function_names.insert(function.name);
                    }
                    Item::Struct(structure) => {
                        type_names.insert(structure.name);
                    }
                    _ => {}
                }
            }
        }
        let start_offset = range
            .map(|r| position_offset(source, r.get("start").unwrap_or(&Value::Null)))
            .unwrap_or(0);
        let end_offset = range
            .map(|r| position_offset(source, r.get("end").unwrap_or(&Value::Null)))
            .unwrap_or(source.len());
        let mut absolute = Vec::<(u32, u32, u32, u32, u32)>::new();
        for (index, token) in tokens.iter().enumerate() {
            let start = token.span.offset();
            let end = start + token.span.len();
            if start < start_offset || end > end_offset || end <= start {
                continue;
            }
            let Some(token_type) = semantic_type(&token.kind, &function_names, &type_names) else {
                continue;
            };
            let start_pos = position_at(source, start);
            let end_pos = position_at(source, end);
            let length = if start_pos[0] == end_pos[0] {
                end_pos[1].saturating_sub(start_pos[1])
            } else {
                token.lexeme.encode_utf16().count() as u32
            };
            let mut modifiers = 0;
            if is_declaration(&tokens, index) {
                modifiers |= 1;
            }
            if matches!(
                token.kind,
                TokenKind::True
                    | TokenKind::False
                    | TokenKind::Integer(_)
                    | TokenKind::Float(_)
                    | TokenKind::String(_)
                    | TokenKind::StringTemplate(_)
            ) {
                modifiers |= 2;
            }
            absolute.push((start_pos[0], start_pos[1], length, token_type, modifiers));
        }
        absolute.sort_by_key(|t| (t.0, t.1));
        let mut data = Vec::with_capacity(absolute.len() * 5);
        let (mut last_line, mut last_start) = (0, 0);
        for (line, start, length, ty, mods) in absolute {
            let delta_line = line - last_line;
            let delta_start = if delta_line == 0 {
                start - last_start
            } else {
                start
            };
            data.extend([delta_line, delta_start, length, ty, mods]);
            last_line = line;
            last_start = start;
        }
        json!({"data":data})
    }

    fn folding_ranges(&self, params: &Value) -> Value {
        let Some((_, source)) = self.source(params) else {
            return json!([]);
        };
        let Ok(tokens) = lex(source) else {
            return json!([]);
        };
        let mut stack = Vec::new();
        let mut ranges = Vec::new();
        for token in tokens {
            match token.kind {
                TokenKind::LeftBrace => stack.push(token.span.offset()),
                TokenKind::RightBrace => {
                    if let Some(start) = stack.pop() {
                        let a = position_at(source, start)[0];
                        let b = position_at(source, token.span.offset())[0];
                        if b > a {
                            ranges.push(json!({"startLine":a,"endLine":b,"kind":"region"}));
                        }
                    }
                }
                _ => {}
            }
        }
        let lines = source.lines().collect::<Vec<_>>();
        let mut start = None;
        for (i, line) in lines.iter().enumerate() {
            if line.trim().is_empty() {
                if start.is_none() {
                    start = Some(i);
                }
            } else if let Some(first) = start.take() {
                if i.saturating_sub(first) > 1 {
                    ranges.push(json!({"startLine":first,"endLine":i-1,"kind":"comment"}));
                }
            }
        }
        json!(ranges)
    }

    fn formatting(&self, params: &Value, range: Option<&Value>) -> Value {
        let Some((_, source)) = self.source(params) else {
            return json!([]);
        };
        let tokens = lex(source).unwrap_or_default();
        let mut brace_delta = vec![0i32; source.lines().count().max(1)];
        for token in &tokens {
            let line = position_at(source, token.span.offset())[0] as usize;
            if let Some(delta) = brace_delta.get_mut(line) {
                match token.kind {
                    TokenKind::LeftBrace => *delta += 1,
                    TokenKind::RightBrace => *delta -= 1,
                    _ => {}
                }
            }
        }
        let range_start = range
            .map(|r| {
                r.get("start")
                    .and_then(|p| p.get("line"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize
            })
            .unwrap_or(0);
        let range_end = range
            .map(|r| {
                r.get("end")
                    .and_then(|p| p.get("line"))
                    .and_then(Value::as_u64)
                    .unwrap_or(usize::MAX as u64) as usize
            })
            .unwrap_or(usize::MAX);
        let original_lines = source.lines().collect::<Vec<_>>();
        let mut depth = 0i32;
        let mut output = Vec::new();
        for (index, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            let leading_close = trimmed.starts_with('}') || trimmed.starts_with(')');
            let indent = (depth - i32::from(leading_close)).max(0) as usize;
            let formatted = if trimmed.is_empty() {
                String::new()
            } else {
                format!("{}{}", "    ".repeat(indent), normalize_spacing(trimmed))
            };
            output.push(if index >= range_start && index <= range_end {
                formatted
            } else {
                line.to_owned()
            });
            depth = (depth + brace_delta.get(index).copied().unwrap_or(0)).max(0);
        }
        let mut formatted = output.join("\n");
        if source.ends_with('\n') {
            formatted.push('\n');
        }
        if formatted == source {
            return json!([]);
        }
        if range.is_some() {
            let mut edits = Vec::new();
            let mut offset = 0usize;
            for (index, line) in output.iter().enumerate() {
                if original_lines
                    .get(index)
                    .is_some_and(|original| *original != line)
                {
                    let original = original_lines[index];
                    edits.push(json!({
                        "range":span_range(source,offset,offset+original.len()),
                        "newText":line
                    }));
                }
                offset += original_lines
                    .get(index)
                    .map(|line| line.len() + 1)
                    .unwrap_or(0);
            }
            return json!(edits);
        }
        let end = position_at(source, source.len());
        json!([{"range":{"start":{"line":0,"character":0},"end":{"line":end[0],"character":end[1]}},"newText":formatted}])
    }

    fn selection_ranges(&self, params: &Value) -> Value {
        let Some((_, source)) = self.source(params) else {
            return json!([]);
        };
        params.get("positions").and_then(Value::as_array).into_iter().flatten().map(|position| {
            let offset=position_offset(source,position); let start=source[..clamp_char_boundary(source,offset)].rfind(|c:char| c.is_whitespace()).map(|i| i+1).unwrap_or(0);
            let end=source[clamp_char_boundary(source,offset)..].find(char::is_whitespace).map(|i| clamp_char_boundary(source,offset)+i).unwrap_or(source.len());
            json!({"range":span_range(source,start,end),"parent":{"range":span_range(source,0,source.len())}})
        }).collect::<Vec<_>>().into()
    }

    fn inlay_hints(&self, params: &Value) -> Value {
        let Some((uri, source)) = self.source(params) else {
            return json!([]);
        };
        let Ok(program) = parse(source) else {
            return json!([]);
        };
        let signatures = program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Function(function) if !function.params.is_empty() => Some((
                    function.name.clone(),
                    function
                        .params
                        .iter()
                        .map(|param| param.name.clone())
                        .collect::<Vec<_>>(),
                )),
                _ => None,
            })
            .collect::<HashMap<_, _>>();
        let tokens = self.tokens_for(uri);
        let range = params.get("range");
        let range_start = range
            .map(|r| position_offset(source, r.get("start").unwrap_or(&Value::Null)))
            .unwrap_or(0);
        let range_end = range
            .map(|r| position_offset(source, r.get("end").unwrap_or(&Value::Null)))
            .unwrap_or(source.len());
        let mut hints = Vec::new();
        for index in 0..tokens.len().saturating_sub(1) {
            let TokenKind::Identifier(name) = &tokens[index].kind else {
                continue;
            };
            if index > 0 && matches!(tokens[index - 1].kind, TokenKind::Fn) {
                continue;
            }
            let Some(parameters) = signatures.get(name) else {
                continue;
            };
            if !matches!(tokens[index + 1].kind, TokenKind::LeftParen) {
                continue;
            }
            let mut argument_starts = Vec::new();
            let mut cursor = index + 2;
            if cursor < tokens.len() && !matches!(tokens[cursor].kind, TokenKind::RightParen) {
                argument_starts.push(cursor);
            }
            let (mut parens, mut braces) = (0i32, 0i32);
            while cursor < tokens.len() {
                match tokens[cursor].kind {
                    TokenKind::LeftParen => parens += 1,
                    TokenKind::RightParen if parens == 0 && braces == 0 => break,
                    TokenKind::RightParen => parens -= 1,
                    TokenKind::LeftBrace => braces += 1,
                    TokenKind::RightBrace => braces -= 1,
                    TokenKind::Comma if parens == 0 && braces == 0 => {
                        if cursor + 1 < tokens.len()
                            && !matches!(tokens[cursor + 1].kind, TokenKind::RightParen)
                        {
                            argument_starts.push(cursor + 1);
                        }
                    }
                    _ => {}
                }
                cursor += 1;
            }
            for (argument, parameter) in argument_starts.into_iter().zip(parameters) {
                let token = &tokens[argument];
                let offset = token.span.offset();
                if offset < range_start || offset > range_end {
                    continue;
                }
                let position = position_at(source, offset);
                hints.push(json!({
                    "position":{"line":position[0],"character":position[1]},
                    "label":format!("{}:",parameter),
                    "kind":2,
                    "paddingRight":true
                }));
            }
        }
        hints.sort_by_key(|hint| {
            (
                hint.pointer("/position/line")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                hint.pointer("/position/character")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            )
        });
        json!(hints)
    }

    fn code_actions(&self, params: &Value) -> Value {
        let Some((_, source)) = self.source(params) else {
            return json!([]);
        };
        let requested = params
            .pointer("/context/only")
            .and_then(Value::as_array)
            .map(|kinds| kinds.iter().filter_map(Value::as_str).collect::<Vec<_>>());
        if requested.as_ref().is_some_and(|kinds| {
            !kinds
                .iter()
                .any(|kind| *kind == "source" || *kind == "source.organizeImports")
        }) {
            return json!([]);
        }

        let lines = source.lines().collect::<Vec<_>>();
        let mut import_indices = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            if line.trim().starts_with("import ") || line.trim().starts_with("from ") {
                import_indices.push(index);
            } else if !line.trim().is_empty() || !import_indices.is_empty() {
                break;
            }
        }
        if import_indices.len() < 2 {
            return json!([]);
        }
        let first = import_indices[0];
        let last = *import_indices.last().unwrap_or(&first);
        let mut imports = lines[first..=last]
            .iter()
            .map(|line| line.trim().to_owned())
            .collect::<Vec<_>>();
        let original = imports.clone();
        imports.sort();
        if imports == original {
            return json!([]);
        }
        let start_offset = lines
            .iter()
            .take(first)
            .map(|line| line.len() + 1)
            .sum::<usize>();
        let end_offset = start_offset
            + lines[first..=last]
                .iter()
                .map(|line| line.len() + 1)
                .sum::<usize>();
        let new_text = format!("{}\n", imports.join("\n"));
        let uri = string(params.get("textDocument").unwrap_or(&Value::Null), "uri");
        json!([{
            "title":"Sort imports",
            "kind":"source.organizeImports",
            "isPreferred":true,
            "edit":{"changes":{uri:[{
                "range":span_range(source,start_offset,end_offset.min(source.len())),
                "newText":new_text
            }]}}
        }])
    }
}

fn diagnostics_for(source: &str, uri: &str) -> Vec<Value> {
    if let Err(error) = lex(source) {
        return vec![diagnostic_from_error(&error, source)];
    }
    if let Err(error) = parse(source) {
        return vec![diagnostic_from_error(&error, source)];
    }
    match Compiler::check(source, &uri_to_path(uri)) {
        Ok(()) => Vec::new(),
        Err(report) => report.chain().find_map(|cause| cause.downcast_ref::<AlacoError>()).map(|error| vec![diagnostic_from_error(error, source)]).unwrap_or_else(|| vec![json!({"range":span_range(source,0,0),"severity":1,"source":"alaco","code":"alaco::internal","message":report.to_string()})]),
    }
}

fn diagnostic_from_error(error: &AlacoError, source: &str) -> Value {
    let diagnostic: &dyn Diagnostic = error;
    let code = diagnostic
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "alaco::error".into());
    let span = diagnostic
        .labels()
        .and_then(|mut labels| labels.next())
        .map(|label| *label.inner());
    let (start, end) = span
        .map(|s| (s.offset(), s.offset() + s.len()))
        .unwrap_or((0, 0));
    json!({"range":span_range(source,start,end),"severity":1,"source":"alaco","code":code,"message":error.to_string(),"data":{"help":error.explanation().map(|(_,fix)|fix)}})
}

fn lex(source: &str) -> Result<Vec<Token>, AlacoError> {
    Lexer::new(source).tokenize()
}
fn parse(source: &str) -> Result<Program, AlacoError> {
    Parser::new(lex(source)?).parse()
}
fn string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}

fn position_at(source: &str, offset: usize) -> [u32; 2] {
    let offset = clamp_char_boundary(source, offset.min(source.len()));
    let before = &source[..offset];
    let line = before.bytes().filter(|b| *b == b'\n').count();
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    [
        line as u32,
        source[line_start..offset].encode_utf16().count() as u32,
    ]
}

fn position_offset(source: &str, position: &Value) -> usize {
    let target_line = position.get("line").and_then(Value::as_u64).unwrap_or(0) as usize;
    let target_character = position
        .get("character")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let mut line = 0;
    let mut start = 0;
    for (index, ch) in source.char_indices() {
        if line == target_line {
            break;
        }
        if ch == '\n' {
            line += 1;
            start = index + 1;
        }
    }
    if line < target_line {
        return source.len();
    }
    let mut units = 0;
    for (relative, ch) in source[start..].char_indices() {
        if ch == '\n' || units >= target_character {
            return start + relative;
        }
        units += ch.len_utf16();
    }
    source.len()
}

fn clamp_char_boundary(source: &str, mut offset: usize) -> usize {
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn normalize_spacing(line: &str) -> String {
    let mut output = String::with_capacity(line.len() + 4);
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if !in_string && ch == '/' && chars.peek() == Some(&'/') {
            output.push(ch);
            output.extend(chars);
            break;
        }
        if ch == '"' && !escaped {
            in_string = !in_string;
        }
        if !in_string
            && ch == '{'
            && output
                .chars()
                .last()
                .is_some_and(|last| !last.is_whitespace())
        {
            output.push(' ');
        }
        output.push(ch);
        if in_string && ch == '\\' && !escaped {
            escaped = true;
        } else {
            escaped = false;
        }
    }
    output
}

fn span_range(source: &str, start: usize, end: usize) -> Value {
    json!({"start":{"line":position_at(source,start)[0],"character":position_at(source,start)[1]},"end":{"line":position_at(source,end)[0],"character":position_at(source,end)[1]}})
}
fn token_at(tokens: &[Token], offset: usize) -> Option<&Token> {
    tokens.iter().find(|token| {
        token.span.offset() <= offset
            && offset <= token.span.offset() + token.span.len()
            && token.span.len() > 0
    })
}

fn is_declaration(tokens: &[Token], index: usize) -> bool {
    if index > 0
        && matches!(
            tokens[index - 1].kind,
            TokenKind::Fn | TokenKind::Struct | TokenKind::Let | TokenKind::Mut | TokenKind::For
        )
    {
        return true;
    }
    if index >= 2 && matches!(tokens[index - 2].kind, TokenKind::Fn) {
        return true;
    }
    false
}

fn semantic_type(
    kind: &TokenKind,
    function_names: &HashSet<String>,
    type_names: &HashSet<String>,
) -> Option<u32> {
    Some(match kind {
        TokenKind::Import
        | TokenKind::From
        | TokenKind::As
        | TokenKind::Fn
        | TokenKind::Let
        | TokenKind::Mut
        | TokenKind::Return
        | TokenKind::Stop
        | TokenKind::Skip
        | TokenKind::Loop
        | TokenKind::Repeat
        | TokenKind::While
        | TokenKind::When
        | TokenKind::Previous
        | TokenKind::For
        | TokenKind::In
        | TokenKind::If
        | TokenKind::Else
        | TokenKind::Match
        | TokenKind::Struct
        | TokenKind::Shell
        | TokenKind::And
        | TokenKind::Or => 5,
        TokenKind::True | TokenKind::False | TokenKind::Integer(_) | TokenKind::Float(_) => 6,
        TokenKind::String(_) | TokenKind::StringTemplate(_) | TokenKind::ShellBlock(_) => 7,
        TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Slash
        | TokenKind::Percent
        | TokenKind::Equal
        | TokenKind::PlusEqual
        | TokenKind::MinusEqual
        | TokenKind::StarEqual
        | TokenKind::SlashEqual
        | TokenKind::EqualEqual
        | TokenKind::Bang
        | TokenKind::BangEqual
        | TokenKind::Less
        | TokenKind::LessEqual
        | TokenKind::Greater
        | TokenKind::GreaterEqual
        | TokenKind::Arrow
        | TokenKind::PipeForward => 8,
        TokenKind::Identifier(name) if function_names.contains(name) => 2,
        TokenKind::Identifier(name) if type_names.contains(name) => 1,
        TokenKind::Identifier(name)
            if matches!(
                name.as_str(),
                "Int" | "Float" | "Bool" | "String" | "Void" | "Shell" | "ShellResult"
            ) =>
        {
            1
        }
        TokenKind::Identifier(_) => 4,
        _ => return None,
    })
}

fn type_name(ty: &Type) -> String {
    match ty {
        Type::Int => "Int".into(),
        Type::Float => "Float".into(),
        Type::Bool => "Bool".into(),
        Type::String => "String".into(),
        Type::Shell => "Shell".into(),
        Type::ShellResult => "ShellResult".into(),
        Type::Void => "Void".into(),
        Type::Named(name) => name.clone(),
    }
}
fn signature(function: &crate::ast::Function) -> String {
    format!(
        "fn {}({}){}",
        function.name,
        function
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, type_name(&p.ty)))
            .collect::<Vec<_>>()
            .join(", "),
        function
            .return_type
            .as_ref()
            .map(|ty| format!(" -> {}", type_name(ty)))
            .unwrap_or_default()
    )
}

fn symbols_for(program: &Program, source: &str) -> Value {
    let mut symbols = Vec::new();
    for item in &program.items {
        match item {
            Item::Function(f) => {
                let range = span_range(source, f.span.start, f.span.end);
                symbols.push(json!({"name":f.name,"kind":12,"range":range,"selectionRange":span_range(source,f.span.start,(f.span.start+f.name.len()).min(f.span.end)),"detail":signature(f)}));
            }
            Item::Struct(s) => {
                let range = span_range(source, s.span.start, s.span.end);
                symbols.push(json!({"name":s.name,"kind":23,"range":range,"selectionRange":span_range(source,s.span.start,(s.span.start+s.name.len()).min(s.span.end)),"children":s.fields.iter().map(|f|json!({"name":f.name,"kind":8,"range":span_range(source,f.span.start,f.span.end),"selectionRange":span_range(source,f.span.start,(f.span.start+f.name.len()).min(f.span.end)),"detail":type_name(&f.ty)})).collect::<Vec<_>>() }));
            }
            Item::Import(_) | Item::Statement(_) => {}
        }
    }
    json!(symbols)
}

fn keyword_help(name: &str) -> Option<&'static str> {
    Some(match name {
        "fn" => "`fn name(params) -> Type { ... }` declares a function.",
        "struct" => "`struct Name { field: Type }` declares a named record type.",
        "let" => {
            "`let name = expression` declares an immutable binding. Use `let mut` for reassignment."
        }
        "mut" => "Marks a `let` binding as mutable: `let mut count = 0`.",
        "return" => {
            "Returns from the current function. Value-returning functions must return on every path."
        }
        "if" => "Runs a block when a boolean condition is true.",
        "else" => "Provides an alternative branch for an `if` statement.",
        "match" => "Selects a block based on a value and its patterns.",
        "loop" => {
            "Starts an unconditional loop. `stop` exits and `skip` advances to the next iteration."
        }
        "repeat" => "Runs a loop a specified number of times.",
        "while" => "Repeats a block while its boolean condition remains true.",
        "for" => "Iterates over the supported iterable expression.",
        "stop" => "Exits the innermost loop.",
        "skip" => "Skips to the next iteration of the innermost loop.",
        "import" => "Imports a module, for example `import std.math;`.",
        "from" => "Imports selected symbols from a module.",
        "as" => "Assigns an alias to an import.",
        "and" => "Boolean conjunction; both operands must be `Bool`.",
        "or" => "Boolean disjunction; both operands must be `Bool`.",
        "true" | "false" => "Boolean literal (`Bool`).",
        "Int" => "Signed 64-bit integer type.",
        "Float" => "Floating-point numeric type.",
        "Bool" => "Boolean type with values `true` and `false`.",
        "String" => "UTF-8 string type.",
        "Void" => "Function result type for functions that do not return a value.",
        _ => return None,
    })
}

fn standard_help(name: &str) -> Option<&'static str> {
    Some(match name {
        "print" => "`print(value)` writes a supported value to standard output without a newline.",
        "println" => "`println(value)` writes a supported value followed by a newline.",
        "eprint" => "`eprint(value)` writes a supported value to standard error without a newline.",
        "eprintln" => {
            "`eprintln(value)` writes a supported value to standard error followed by a newline."
        }
        "math.gcd" => {
            "`math.gcd(left: Int, right: Int) -> Int` returns the greatest common divisor."
        }
        "math.lcm" => "`math.lcm(left: Int, right: Int) -> Int` returns the least common multiple.",
        "math.pow_int" => {
            "`math.pow_int(base: Int, exponent: Int) -> Int` computes integer exponentiation."
        }
        "math.is_prime" => "`math.is_prime(value: Int) -> Bool` tests primality.",
        "string.concat" => {
            "`string.concat(left: String, right: String) -> String` concatenates two strings."
        }
        "string.repeat_string" => {
            "`string.repeat_string(value: String, count: Int) -> String` repeats a string."
        }
        "fs.read_text" => {
            "`fs.read_text(path: String) -> String` reads a text file; an empty string may also indicate failure."
        }
        "fs.write_text" => {
            "`fs.write_text(path: String, contents: String) -> Bool` writes a file and returns success."
        }
        "env.get" => {
            "`env.get(name: String) -> String` reads an environment variable; missing values return an empty string."
        }
        "path.join" => {
            "`path.join(left: String, right: String) -> String` joins path components using host path rules."
        }
        _ => return None,
    })
}

fn collect_aco_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some(".git" | "target" | "node_modules")
            ) {
                collect_aco_files(&path, files);
            }
        } else if file_type.is_file()
            && path.extension().is_some_and(|extension| extension == "aco")
        {
            files.push(path);
        }
    }
}

fn uri_to_path(uri: &str) -> String {
    if let Some(path) = uri.strip_prefix("file://") {
        percent_decode(path)
    } else {
        uri.to_owned()
    }
}

fn path_to_uri(path: &Path) -> String {
    let path = path
        .to_string_lossy()
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23");
    format!("file://{path}")
}
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_positions_use_utf16_columns() {
        let source = "a🦀b\nnext";
        assert_eq!(position_at(source, "a🦀".len()), [0, 3]);
        assert_eq!(
            position_offset(source, &json!({"line":0,"character":3})),
            "a🦀".len()
        );
    }

    #[test]
    fn diagnostics_use_the_existing_lexer_parser_and_analyzer() {
        let syntax = diagnostics_for("fn main( { }", "file:///test.aco");
        assert!(!syntax.is_empty());
        assert_eq!(syntax[0]["source"], "alaco");
        let semantic = diagnostics_for("fn main() { missing(); }", "file:///test.aco");
        assert!(
            !semantic.is_empty(),
            "semantic diagnostics should be published: {semantic:?}"
        );
        assert_eq!(semantic[0]["severity"], 1);
    }

    #[test]
    fn formatter_preserves_comments_and_indents_blocks() {
        let mut server = Server::default();
        let uri = "file:///test.aco";
        server.documents.insert(
            uri.into(),
            Document {
                text: "fn main(){\nprintln(1); // keep me\n}\n".into(),
                version: 1,
            },
        );
        let edits = server.formatting(&json!({"textDocument":{"uri":uri}}), None);
        assert!(edits[0]["newText"].as_str().unwrap().contains("// keep me"));
        assert!(
            edits[0]["newText"]
                .as_str()
                .unwrap()
                .contains("    println(1);")
        );
        assert!(
            edits[0]["newText"]
                .as_str()
                .unwrap()
                .contains("fn main() {")
        );
        assert_eq!(
            normalize_spacing("println(\"literal){\");"),
            "println(\"literal){\");"
        );
    }

    #[test]
    fn semantic_token_data_is_lsp_delta_encoded() {
        let mut server = Server::default();
        server.documents.insert(
            "file:///test.aco".into(),
            Document {
                text: "fn main() { println(1); }".into(),
                version: 1,
            },
        );
        let value =
            server.semantic_tokens(&json!({"textDocument":{"uri":"file:///test.aco"}}), None);
        let data = value["data"].as_array().unwrap();
        assert!(!data.is_empty());
        assert_eq!(data.len() % 5, 0);
    }

    #[test]
    fn completions_include_language_and_file_symbols() {
        let mut server = Server::default();
        let uri = "file:///test.aco";
        server.documents.insert(
            uri.into(),
            Document {
                text: "fn calculate(value: Int) -> Int { return value; }\nfn main() {}".into(),
                version: 1,
            },
        );
        let result = server.completions(&json!({
            "textDocument":{"uri":uri},"position":{"line":1,"character":0}
        }));
        let labels = result["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["label"].as_str())
            .collect::<Vec<_>>();
        assert!(labels.contains(&"calculate"));
        assert!(labels.contains(&"println"));
        assert!(labels.contains(&"std.math"));
    }

    #[test]
    fn definitions_and_rename_use_open_document_tokens() {
        let mut server = Server::default();
        let uri = "file:///test.aco";
        server.documents.insert(
            uri.into(),
            Document {
                text: "fn greet() {}\nfn main() { greet(); }".into(),
                version: 1,
            },
        );
        let definition = server.definition(&json!({
            "textDocument":{"uri":uri},"position":{"line":1,"character":13}
        }));
        assert_eq!(definition["uri"], uri);
        assert_eq!(
            definition
                .pointer("/range/start/line")
                .and_then(Value::as_u64),
            Some(0)
        );
        let edit = server.rename(&json!({
            "textDocument":{"uri":uri},"position":{"line":1,"character":13},"newName":"welcome"
        }));
        assert_eq!(edit["changes"][uri][0]["newText"].as_str(), Some("welcome"));
        assert_eq!(edit["changes"][uri][1]["newText"].as_str(), Some("welcome"));
    }

    #[test]
    fn code_action_sorts_imports_without_dropping_duplicates() {
        let mut server = Server::default();
        let uri = "file:///test.aco";
        server.documents.insert(
            uri.into(),
            Document {
                text: "import std.string;\nimport std.math;\nfn main() {}\n".into(),
                version: 1,
            },
        );
        let actions = server.code_actions(
            &json!({"textDocument":{"uri":uri},"context":{"only":["source.organizeImports"]}}),
        );
        assert_eq!(actions[0]["kind"], "source.organizeImports");
        assert_eq!(
            actions[0]["edit"]["changes"][uri][0]["newText"].as_str(),
            Some("import std.math;\nimport std.string;\n")
        );
    }

    #[test]
    fn inlay_hints_show_parameter_names_for_local_calls() {
        let mut server = Server::default();
        let uri = "file:///test.aco";
        server.documents.insert(uri.into(), Document {
            text: "fn add(left: Int, right: Int) -> Int { return left + right; }\nfn main() { add(1, 2); }".into(),
            version: 1,
        });
        let hints = server.inlay_hints(&json!({"textDocument":{"uri":uri}}));
        assert_eq!(hints.as_array().unwrap().len(), 2);
        assert_eq!(hints[0]["label"], "left:");
        assert_eq!(hints[1]["label"], "right:");
    }

    #[test]
    fn code_lens_can_check_the_current_document() {
        let mut server = Server::default();
        let uri = "file:///test.aco";
        server.documents.insert(
            uri.into(),
            Document {
                text: "fn first() {}\nfn second() {}".into(),
                version: 1,
            },
        );
        let lenses = server.code_lenses(&json!({"textDocument":{"uri":uri}}));
        assert_eq!(lenses.as_array().unwrap().len(), 1);
        assert_eq!(lenses[0]["command"]["command"], "alaco.checkDocument");
        assert_eq!(lenses[0]["command"]["arguments"][0], uri);
        assert!(
            lenses[0]["command"]["title"]
                .as_str()
                .unwrap()
                .contains("2 functions")
        );
    }

    #[test]
    fn completion_resolve_adds_markdown_documentation() {
        let server = Server::default();
        let item = json!({"label":"math.gcd","detail":"Greatest common divisor"});
        let resolved = server.resolve_completion(&item);
        assert_eq!(resolved["documentation"]["kind"], "markdown");
        assert!(
            resolved["documentation"]["value"]
                .as_str()
                .unwrap()
                .contains("greatest common divisor")
        );
    }

    #[test]
    fn document_links_resolve_existing_workspace_modules() {
        let root = std::env::temp_dir().join(format!("alaco-lsp-links-{}", std::process::id()));
        let module = root.join("std/std/math.aco");
        std::fs::create_dir_all(module.parent().unwrap()).unwrap();
        std::fs::write(&module, "fn gcd() {}\n").unwrap();
        let mut server = Server::default();
        server.root_uri = Some(path_to_uri(&root));
        let uri = path_to_uri(&root.join("main.aco"));
        server.documents.insert(
            uri.clone(),
            Document {
                text: "import std.math;\nfn main() {}\n".into(),
                version: 1,
            },
        );
        let links = server.document_links(&json!({"textDocument":{"uri":uri}}));
        assert_eq!(links.as_array().unwrap().len(), 1);
        assert_eq!(links[0]["target"], path_to_uri(&module));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn workspace_navigation_includes_unopened_aco_files() {
        let root = std::env::temp_dir().join(format!("alaco-lsp-workspace-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let main_path = root.join("main.aco");
        let helper_path = root.join("helpers.aco");
        let main_source = "fn main() { helper(); }";
        std::fs::write(&main_path, main_source).unwrap();
        std::fs::write(&helper_path, "fn helper() -> Int { return 7; }\n").unwrap();

        let mut server = Server::default();
        server.root_uri = Some(path_to_uri(&root));
        let main_uri = path_to_uri(&main_path);
        let helper_uri = path_to_uri(&helper_path);
        server.documents.insert(
            main_uri.clone(),
            Document {
                text: main_source.into(),
                version: 1,
            },
        );

        let definition = server.definition(&json!({
            "textDocument":{"uri":main_uri},"position":{"line":0,"character":14}
        }));
        assert_eq!(definition["uri"], helper_uri);

        let symbols = server.workspace_symbols(&json!({"query":"helper"}));
        assert_eq!(symbols.as_array().unwrap().len(), 1);
        assert_eq!(symbols[0]["location"]["uri"], helper_uri);

        let rename = server.rename(&json!({
            "textDocument":{"uri":main_uri},"position":{"line":0,"character":14},"newName":"assist"
        }));
        assert_eq!(rename["changes"][&helper_uri].as_array().unwrap().len(), 1);
        assert_eq!(rename["changes"][&main_uri].as_array().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(root);
    }
}
