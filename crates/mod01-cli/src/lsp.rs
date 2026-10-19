use std::{
    collections::BTreeMap,
    io::{self, BufRead, BufReader, Write},
    process::ExitCode,
};

use modl_core::{
    AssetCatalog, Diagnostic, FileId, SourceFile, Span, TokenKind, analyze_module,
    builtin_definitions, format_source, lex, parse,
};
use serde_json::{Value, json};

const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const KEYWORDS: &[&str] = &[
    "and", "as", "assert", "break", "case", "const", "continue", "draw", "elif", "else", "enum",
    "false", "fn", "for", "if", "import", "in", "let", "match", "none", "not", "on", "or",
    "private", "pub", "raster", "record", "return", "start", "state", "task", "true", "update",
    "var", "wait", "while",
];
#[derive(Debug)]
struct Document {
    text: String,
    version: i64,
}

#[derive(Debug, Default)]
struct Server {
    documents: BTreeMap<String, Document>,
    shutdown_requested: bool,
}

pub fn run() -> ExitCode {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    let mut server = Server::default();
    loop {
        match read_message(&mut reader) {
            Ok(Some(message)) => {
                let should_exit = server.handle(&message, &mut writer);
                if should_exit {
                    return if server.shutdown_requested {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    };
                }
            }
            Ok(None) => return ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("mod01 lsp: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
}

impl Server {
    fn handle(&mut self, message: &Value, writer: &mut impl Write) -> bool {
        let method = message.get("method").and_then(Value::as_str);
        let id = message.get("id");
        match method {
            Some("initialize") => respond(
                writer,
                id,
                &json!({
                    "capabilities": {
                        "textDocumentSync": 1,
                        "completionProvider": { "triggerCharacters": ["#"] },
                        "hoverProvider": true,
                        "definitionProvider": true,
                        "referencesProvider": true,
                        "renameProvider": { "prepareProvider": false },
                        "signatureHelpProvider": { "triggerCharacters": ["(", ","] },
                        "documentSymbolProvider": true,
                        "workspaceSymbolProvider": true,
                        "documentFormattingProvider": true
                    },
                    "serverInfo": { "name": "mod01", "version": env!("CARGO_PKG_VERSION") }
                }),
            ),
            Some("initialized") => {}
            Some("shutdown") => {
                self.shutdown_requested = true;
                respond(writer, id, &Value::Null);
            }
            Some("exit") => return true,
            Some("textDocument/didOpen") => self.did_open(message, writer),
            Some("textDocument/didChange") => self.did_change(message, writer),
            Some("textDocument/didClose") => self.did_close(message, writer),
            Some("textDocument/completion") => {
                respond(writer, id, &self.completion_items());
            }
            Some("textDocument/hover") => {
                let result = self.symbol_request(message).and_then(|target| {
                    hover_text(&target.name).map(|contents| json!({ "contents": contents }))
                });
                respond(writer, id, &result.unwrap_or(Value::Null));
            }
            Some("textDocument/definition") => {
                let result = self.symbol_request(message).and_then(|target| {
                    let source = self.source(&target.uri)?;
                    definition_span(&source, &target.name)
                        .map(|span| location(&target.uri, &source, span))
                });
                respond(writer, id, &result.unwrap_or(Value::Null));
            }
            Some("textDocument/references") => {
                let result = self
                    .symbol_request(message)
                    .map(|target| self.reference_locations(&target))
                    .unwrap_or_default();
                respond(writer, id, &json!(result));
            }
            Some("textDocument/rename") => {
                let new_name = message
                    .pointer("/params/newName")
                    .and_then(Value::as_str)
                    .filter(|name| valid_identifier(name));
                let result = self.symbol_request(message).and_then(|target| {
                    new_name.map(|name| {
                        let mut changes = BTreeMap::<String, Vec<Value>>::new();
                        for (uri, source, span) in self.reference_spans(&target) {
                            changes
                                .entry(uri)
                                .or_default()
                                .push(json!({ "range": range(&source, span), "newText": name }));
                        }
                        json!({ "changes": changes })
                    })
                });
                respond(writer, id, &result.unwrap_or(Value::Null));
            }
            Some("textDocument/signatureHelp") => {
                let result = self.signature_help(message).unwrap_or(Value::Null);
                respond(writer, id, &result);
            }
            Some("textDocument/documentSymbol") => {
                let uri = message
                    .pointer("/params/textDocument/uri")
                    .and_then(Value::as_str);
                let result = uri.map_or_else(Vec::new, |uri| self.document_symbols(uri));
                respond(writer, id, &json!(result));
            }
            Some("workspace/symbol") => {
                let query = message
                    .pointer("/params/query")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                respond(writer, id, &json!(self.workspace_symbols(query)));
            }
            Some("textDocument/formatting") => {
                respond(writer, id, &json!(self.formatting_edits(message)));
            }
            Some(method) if id.is_some() => send_error(writer, id, -32601, method),
            _ => {}
        }
        false
    }

    fn did_open(&mut self, message: &Value, writer: &mut impl Write) {
        let Some(uri) = message
            .pointer("/params/textDocument/uri")
            .and_then(Value::as_str)
        else {
            return;
        };
        let Some(text) = message
            .pointer("/params/textDocument/text")
            .and_then(Value::as_str)
        else {
            return;
        };
        let version = message
            .pointer("/params/textDocument/version")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        self.documents.insert(
            uri.to_owned(),
            Document {
                text: text.to_owned(),
                version,
            },
        );
        self.publish_affected_diagnostics(writer, uri);
    }

    fn did_change(&mut self, message: &Value, writer: &mut impl Write) {
        let Some(uri) = message
            .pointer("/params/textDocument/uri")
            .and_then(Value::as_str)
        else {
            return;
        };
        let Some(text) = message
            .pointer("/params/contentChanges/0/text")
            .and_then(Value::as_str)
        else {
            return;
        };
        let version = message
            .pointer("/params/textDocument/version")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        self.documents.insert(
            uri.to_owned(),
            Document {
                text: text.to_owned(),
                version,
            },
        );
        self.publish_affected_diagnostics(writer, uri);
    }

    fn did_close(&mut self, message: &Value, writer: &mut impl Write) {
        let Some(uri) = message
            .pointer("/params/textDocument/uri")
            .and_then(Value::as_str)
        else {
            return;
        };
        self.documents.remove(uri);
        notify(
            writer,
            "textDocument/publishDiagnostics",
            &json!({ "uri": uri, "diagnostics": [] }),
        );
        for dependent in self.dependents_of(uri) {
            self.publish_diagnostics(writer, &dependent);
        }
    }

    fn publish_diagnostics(&self, writer: &mut impl Write, uri: &str) {
        let Some(document) = self.documents.get(uri) else {
            return;
        };
        let source = SourceFile::new(FileId(0), uri, document.text.clone());
        let diagnostics = analyze_module(&source, &AssetCatalog::default())
            .diagnostics
            .iter()
            .filter(|diagnostic| !self.resolved_import_diagnostic(&source, diagnostic))
            .map(|diagnostic| lsp_diagnostic(&source, diagnostic))
            .collect::<Vec<_>>();
        notify(
            writer,
            "textDocument/publishDiagnostics",
            &json!({ "uri": uri, "version": document.version, "diagnostics": diagnostics }),
        );
    }

    fn publish_affected_diagnostics(&self, writer: &mut impl Write, uri: &str) {
        self.publish_diagnostics(writer, uri);
        for dependent in self.dependents_of(uri) {
            self.publish_diagnostics(writer, &dependent);
        }
    }

    fn dependents_of(&self, target_uri: &str) -> Vec<String> {
        self.documents
            .keys()
            .filter(|uri| uri.as_str() != target_uri)
            .filter_map(|uri| {
                let source = self.source(uri)?;
                import_aliases(&source)
                    .values()
                    .any(|path| target_uri.ends_with(&format!("/{path}")))
                    .then(|| uri.clone())
            })
            .collect()
    }

    fn resolved_import_diagnostic(&self, source: &SourceFile, diagnostic: &Diagnostic) -> bool {
        if diagnostic.code != "M013007" {
            return false;
        }
        let aliases = import_aliases(source);
        lex(source).tokens.windows(3).any(|window| {
            let TokenKind::Identifier(alias) = &window[0].kind else {
                return false;
            };
            let TokenKind::Identifier(member) = &window[2].kind else {
                return false;
            };
            matches!(window[1].kind, TokenKind::Dot)
                && window[0].span.start <= diagnostic.primary.span.start
                && diagnostic.primary.span.end <= window[2].span.end
                && aliases
                    .get(alias)
                    .and_then(|path| self.import_uri(path))
                    .and_then(|uri| self.source(&uri))
                    .is_some_and(|target| public_declaration(&target, member))
        })
    }

    fn source(&self, uri: &str) -> Option<SourceFile> {
        self.documents
            .get(uri)
            .map(|document| SourceFile::new(FileId(0), uri, document.text.clone()))
    }

    fn formatting_edits(&self, message: &Value) -> Vec<Value> {
        message
            .pointer("/params/textDocument/uri")
            .and_then(Value::as_str)
            .and_then(|uri| self.source(uri))
            .and_then(|source| format_source(&source).ok().map(|text| (source, text)))
            .map(|(source, text)| {
                vec![json!({
                    "range": range(&source, Span::new(source.id(), 0, source.eof_span().end)),
                    "newText": text
                })]
            })
            .unwrap_or_default()
    }

    fn import_uri(&self, path: &str) -> Option<String> {
        let suffix = format!("/{path}");
        self.documents
            .keys()
            .find(|uri| uri.ends_with(&suffix))
            .cloned()
    }

    fn reference_spans(&self, target: &SymbolTarget) -> Vec<(String, SourceFile, Span)> {
        let mut result = Vec::new();
        if let Some(source) = self.source(&target.uri) {
            result.extend(
                identifier_spans(&source, &target.name)
                    .into_iter()
                    .map(|span| (target.uri.clone(), source.clone(), span)),
            );
        }
        for uri in self.documents.keys().filter(|uri| **uri != target.uri) {
            let Some(source) = self.source(uri) else {
                continue;
            };
            let aliases = import_aliases(&source);
            let imported_aliases: Vec<_> = aliases
                .iter()
                .filter_map(|(alias, path)| {
                    (self.import_uri(path).as_deref() == Some(target.uri.as_str()))
                        .then_some(alias.as_str())
                })
                .collect();
            if imported_aliases.is_empty() {
                continue;
            }
            let tokens = lex(&source).tokens;
            for window in tokens.windows(3) {
                if matches!(&window[0].kind, TokenKind::Identifier(alias) if imported_aliases.contains(&alias.as_str()))
                    && matches!(window[1].kind, TokenKind::Dot)
                    && matches!(&window[2].kind, TokenKind::Identifier(name) if name == &target.name)
                {
                    result.push((uri.clone(), source.clone(), window[2].span));
                }
            }
        }
        result
    }

    fn reference_locations(&self, target: &SymbolTarget) -> Vec<Value> {
        self.reference_spans(target)
            .into_iter()
            .map(|(uri, source, span)| location(&uri, &source, span))
            .collect()
    }

    fn completion_items(&self) -> Value {
        let mut items = completion_items().as_array().cloned().unwrap_or_default();
        let mut seen: std::collections::BTreeSet<String> = items
            .iter()
            .filter_map(|item| item.get("label").and_then(Value::as_str).map(str::to_owned))
            .collect();
        for uri in self.documents.keys() {
            let Some(source) = self.source(uri) else {
                continue;
            };
            for (name, _, kind) in declaration_spans(&source) {
                if seen.insert(name.clone()) {
                    items.push(
                        json!({ "label": name, "kind": kind, "detail": "MODL project symbol" }),
                    );
                }
            }
        }
        Value::Array(items)
    }

    fn document_symbols(&self, uri: &str) -> Vec<Value> {
        let Some(source) = self.source(uri) else {
            return Vec::new();
        };
        declaration_spans(&source)
            .into_iter()
            .map(|(name, span, kind)| {
                json!({
                    "name": name, "kind": kind, "range": range(&source, span),
                    "selectionRange": range(&source, span)
                })
            })
            .collect()
    }

    fn workspace_symbols(&self, query: &str) -> Vec<Value> {
        let query = query.to_ascii_lowercase();
        self.documents
            .keys()
            .flat_map(|uri| {
                let Some(source) = self.source(uri) else {
                    return Vec::new();
                };
                declaration_spans(&source)
                    .into_iter()
                    .filter(|(name, _, _)| name.to_ascii_lowercase().contains(&query))
                    .map(|(name, span, kind)| {
                        json!({ "name": name, "kind": kind, "location": location(uri, &source, span) })
                    })
                    .collect()
            })
            .collect()
    }

    fn signature_help(&self, message: &Value) -> Option<Value> {
        let uri = message.pointer("/params/textDocument/uri")?.as_str()?;
        let source = self.source(uri)?;
        let line = u32::try_from(message.pointer("/params/position/line")?.as_u64()?).ok()?;
        let character =
            u32::try_from(message.pointer("/params/position/character")?.as_u64()?).ok()?;
        let offset = offset_at(&source, line, character)?;
        let prefix = source.text().get(..usize::try_from(offset).ok()?)?;
        let open = prefix.rfind('(')?;
        let callee = prefix[..open]
            .trim_end()
            .rsplit(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .next()?;
        let active_parameter = prefix[open + 1..]
            .bytes()
            .filter(|byte| *byte == b',')
            .count();
        let label = builtin_definitions()
            .iter()
            .find(|definition| definition.name == callee)
            .map(|definition| definition.signature.clone())
            .or_else(|| self.declaration_label(callee))?;
        Some(json!({
            "signatures": [{ "label": label }],
            "activeSignature": 0,
            "activeParameter": active_parameter
        }))
    }

    fn declaration_label(&self, name: &str) -> Option<String> {
        for document in self.documents.values() {
            for line in document.text.lines() {
                let declaration = line
                    .trim_start_matches([' ', '\t'])
                    .strip_prefix("pub ")
                    .or_else(|| {
                        line.trim_start_matches([' ', '\t'])
                            .strip_prefix("private ")
                    })
                    .unwrap_or_else(|| line.trim_start_matches([' ', '\t']));
                if (declaration.starts_with(&format!("fn {name}("))
                    || declaration.starts_with(&format!("task {name}(")))
                    && let Some(header) = declaration.strip_suffix(':')
                {
                    return Some(header.to_owned());
                }
            }
        }
        None
    }

    fn symbol_request(&self, message: &Value) -> Option<SymbolTarget> {
        let uri = message
            .pointer("/params/textDocument/uri")?
            .as_str()?
            .to_owned();
        let document = self.documents.get(&uri)?;
        let source = SourceFile::new(FileId(0), uri.clone(), document.text.clone());
        let line = u32::try_from(message.pointer("/params/position/line")?.as_u64()?).ok()?;
        let character =
            u32::try_from(message.pointer("/params/position/character")?.as_u64()?).ok()?;
        let offset = offset_at(&source, line, character)?;
        let tokens = lex(&source).tokens;
        let token_index = tokens.iter().position(|token| {
            if (token.span.contains(offset)
                || (token.span.end == offset && token.span.start < offset))
                && matches!(token.kind, TokenKind::Identifier(_))
            {
                return true;
            }
            false
        })?;
        let TokenKind::Identifier(name) = &tokens[token_index].kind else {
            return None;
        };
        if token_index >= 2
            && matches!(tokens[token_index - 1].kind, TokenKind::Dot)
            && let TokenKind::Identifier(alias) = &tokens[token_index - 2].kind
            && let Some(path) = import_aliases(&source).get(alias)
            && let Some(import_uri) = self.import_uri(path)
        {
            return Some(SymbolTarget {
                uri: import_uri,
                name: name.clone(),
            });
        }
        Some(SymbolTarget {
            uri,
            name: name.clone(),
        })
    }
}

#[derive(Clone, Debug)]
struct SymbolTarget {
    uri: String,
    name: String,
}

fn respond(writer: &mut impl Write, id: Option<&Value>, result: &Value) {
    let Some(id) = id else {
        return;
    };
    write_message(
        writer,
        &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
    );
}

fn send_error(writer: &mut impl Write, id: Option<&Value>, code: i32, method: &str) {
    let Some(id) = id else {
        return;
    };
    write_message(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": format!("unsupported LSP method '{method}'") }
        }),
    );
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if bytes == 0 {
            return if content_length.is_none() {
                Ok(None)
            } else {
                Err("unexpected EOF in LSP headers".to_owned())
            };
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "invalid LSP Content-Length".to_owned())?,
            );
        }
    }
    let length = content_length.ok_or_else(|| "missing LSP Content-Length".to_owned())?;
    if length > MAX_MESSAGE_BYTES {
        return Err("LSP message exceeds 8 MiB".to_owned());
    }
    let mut bytes = vec![0_u8; length];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn write_message(writer: &mut impl Write, value: &Value) {
    let Ok(bytes) = serde_json::to_vec(value) else {
        return;
    };
    if write!(writer, "Content-Length: {}\r\n\r\n", bytes.len()).is_ok()
        && writer.write_all(&bytes).is_ok()
    {
        let _ = writer.flush();
    }
}

fn notify(writer: &mut impl Write, method: &str, params: &Value) {
    write_message(
        writer,
        &json!({ "jsonrpc": "2.0", "method": method, "params": params }),
    );
}

fn completion_items() -> Value {
    let keywords = KEYWORDS
        .iter()
        .map(|label| json!({ "label": label, "kind": 14 }));
    let builtins = builtin_definitions().iter().map(|definition| {
        json!({ "label": definition.name, "kind": if definition.callable { 3 } else { 21 },
            "detail": definition.signature })
    });
    Value::Array(keywords.chain(builtins).collect())
}

fn hover_text(symbol: &str) -> Option<Value> {
    if let Some(definition) = builtin_definitions()
        .iter()
        .find(|definition| definition.name == symbol)
    {
        return Some(
            json!({ "kind": "markdown", "value": format!("\x60\x60\x60modl\n{}\n\x60\x60\x60", definition.signature) }),
        );
    }
    KEYWORDS
        .contains(&symbol)
        .then(|| json!({ "kind": "plaintext", "value": format!("MODL/1 keyword `{symbol}`") }))
        .or_else(|| {
            Some(json!({ "kind": "plaintext", "value": format!("MODL symbol `{symbol}`") }))
        })
}

fn definition_span(source: &SourceFile, symbol: &str) -> Option<Span> {
    let tokens = lex(source).tokens;
    for pair in tokens.windows(2) {
        if is_definition_prefix(&pair[0].kind)
            && matches!(&pair[1].kind, TokenKind::Identifier(name) if name == symbol)
        {
            return Some(pair[1].span);
        }
    }
    for window in tokens.windows(3) {
        if matches!(&window[0].kind, TokenKind::LeftParen | TokenKind::Comma)
            && matches!(&window[1].kind, TokenKind::Identifier(name) if name == symbol)
            && matches!(window[2].kind, TokenKind::Colon)
        {
            return Some(window[1].span);
        }
    }
    None
}

fn import_aliases(source: &SourceFile) -> BTreeMap<String, String> {
    parse(source)
        .module
        .items
        .into_iter()
        .filter_map(|item| {
            let modl_core::ast::Item::Import(import) = item else {
                return None;
            };
            let alias = import
                .alias
                .map(|name| name.value)
                .or_else(|| import.path.last().map(|name| name.value.clone()))?;
            let path = format!(
                "{}.modl",
                import
                    .path
                    .iter()
                    .map(|part| part.value.as_str())
                    .collect::<Vec<_>>()
                    .join("/")
            );
            Some((alias, path))
        })
        .collect()
}

fn declaration_spans(source: &SourceFile) -> Vec<(String, Span, u8)> {
    let tokens = lex(source).tokens;
    tokens
        .windows(2)
        .filter_map(|pair| {
            let kind = match pair[0].kind {
                TokenKind::Fn | TokenKind::Task => 12,
                TokenKind::Record => 23,
                TokenKind::Enum => 10,
                TokenKind::Const | TokenKind::State => 13,
                _ => return None,
            };
            let TokenKind::Identifier(name) = &pair[1].kind else {
                return None;
            };
            Some((name.clone(), pair[1].span, kind))
        })
        .collect()
}

fn public_declaration(source: &SourceFile, name: &str) -> bool {
    use modl_core::ast::{Item, Visibility};
    parse(source).module.items.iter().any(|item| match item {
        Item::Constant(value) => value.visibility == Visibility::Public && value.name.value == name,
        Item::State(value) => value.visibility == Visibility::Public && value.name.value == name,
        Item::Function(value) => value.visibility == Visibility::Public && value.name.value == name,
        Item::Task(value) => value.visibility == Visibility::Public && value.name.value == name,
        Item::Record(value) => value.visibility == Visibility::Public && value.name.value == name,
        Item::Enum(value) => value.visibility == Visibility::Public && value.name.value == name,
        Item::Import(_) | Item::Callback(_) | Item::Assertion(_) => false,
    })
}

fn is_definition_prefix(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Const
            | TokenKind::State
            | TokenKind::Let
            | TokenKind::Var
            | TokenKind::Fn
            | TokenKind::Task
            | TokenKind::Record
            | TokenKind::Enum
    )
}

fn identifier_spans(source: &SourceFile, symbol: &str) -> Vec<Span> {
    lex(source)
        .tokens
        .into_iter()
        .filter_map(|token| match token.kind {
            TokenKind::Identifier(name) if name == symbol => Some(token.span),
            _ => None,
        })
        .collect()
}

fn lsp_diagnostic(source: &SourceFile, diagnostic: &Diagnostic) -> Value {
    json!({
        "range": range(source, diagnostic.primary.span),
        "severity": 1,
        "code": diagnostic.code,
        "source": "modl",
        "message": diagnostic.message
    })
}

fn location(uri: &str, source: &SourceFile, span: Span) -> Value {
    json!({ "uri": uri, "range": range(source, span) })
}

fn range(source: &SourceFile, span: Span) -> Value {
    let start = source.line_column(span.start);
    let end = source.line_column(span.end);
    json!({
        "start": { "line": start.line, "character": start.column },
        "end": { "line": end.line, "character": end.column }
    })
}

fn offset_at(source: &SourceFile, line: u32, character: u32) -> Option<u32> {
    let mut offset = 0_u32;
    for (index, text_line) in source.text().split_inclusive('\n').enumerate() {
        if u32::try_from(index).ok()? == line {
            let content_length = u32::try_from(text_line.trim_end_matches('\n').len()).ok()?;
            return (character <= content_length).then_some(offset + character);
        }
        offset = offset.checked_add(u32::try_from(text_line.len()).ok()?)?;
    }
    (line == 0 && source.text().is_empty() && character == 0).then_some(0)
}

fn valid_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && !KEYWORDS.contains(&value)
}
