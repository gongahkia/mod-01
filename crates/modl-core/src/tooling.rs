//! Original-source navigation and dependency relationships derived from resolved compiler IR.
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{
    AnalysisOutput, Diagnostic, FileId, SourceFile, Span, Type,
    codegen::SourceOriginMap,
    ir::{
        IrExpression, IrExpressionKind, IrPlace, IrStatement, IrStatementKind, Symbol, SymbolKind,
    },
    types::SymbolId,
};

#[derive(Clone, Debug, Serialize)]
pub struct ProjectSource {
    pub id: FileId,
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectSymbol {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub detail: String,
    pub defined_at: Option<Span>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Read,
    Write,
    Call,
    Start,
    Asset,
}

#[derive(Clone, Debug, Serialize)]
pub struct SymbolReference {
    pub symbol: SymbolId,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub span: Option<Span>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub kind: RelationshipKind,
    pub span: Span,
}

/// Read-only compiler model, never a hand-authored scene or execution engine.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ProjectModel {
    pub sources: Vec<ProjectSource>,
    pub symbols: Vec<ProjectSymbol>,
    pub diagnostics: Vec<Diagnostic>,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub references: Vec<SymbolReference>,
}

/// Source analysis, not runnable-game admission. Member keys scope all source,
/// symbol and node IDs; equal local IDs in two cartridges are never merged.
#[derive(Clone, Debug, Serialize)]
pub struct GameModel {
    pub revision: u8,
    pub entry: &'static str,
    pub members: BTreeMap<String, GameMemberModel>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameMemberModel {
    pub cartridge_id: String,
    pub model: ProjectModel,
}

/// Derives each member's graph from its own resolved compiler IR. Invalid MODL
/// drafts retain original-source diagnostics without executing or packing code.
///
/// # Errors
/// Refuses invalid current workcart structure/metadata, duplicate persistent IDs,
/// and member-qualified linking errors. This never imports JavaScript or reads paths.
pub fn analyze_game_model(source: &str) -> Result<GameModel, crate::CartridgeError> {
    let main = crate::parse_workcart(source)?;
    let mut projects = BTreeMap::new();
    for member in &main.bundle {
        projects.insert(member.alias.clone(), crate::parse_workcart(&member.source)?);
    }
    projects.insert(crate::bundle::BUNDLE_ENTRY.to_owned(), main);
    let mut identities = BTreeSet::new();
    let mut members = BTreeMap::new();
    for (alias, project) in projects {
        if !identities.insert(project.manifest.id.clone()) {
            return Err(crate::CartridgeError {
                code: "M014071",
                message: format!(
                    "bundle cartridge '{alias}' reuses save identity '{}'; every member needs its own cartridge id",
                    project.manifest.id
                ),
            });
        }
        let analyzed = (|| {
            let metadata =
                toml::to_string(&project.manifest).map_err(|error| crate::CartridgeError {
                    code: "M014020",
                    message: error.to_string(),
                })?;
            crate::analyze_project_model(&metadata, &project.files)
        })()
        .map_err(|mut error| {
            error.message = format!("bundle cartridge '{alias}': {}", error.message);
            error
        })?;
        members.insert(
            alias,
            GameMemberModel {
                cartridge_id: project.manifest.id,
                model: analyzed,
            },
        );
    }
    Ok(GameModel {
        revision: 1,
        entry: crate::bundle::BUNDLE_ENTRY,
        members,
    })
}

impl ProjectModel {
    #[must_use]
    pub fn source(&self, file: FileId) -> Option<&ProjectSource> {
        self.sources.iter().find(|source| source.id == file)
    }

    /// Chooses the smallest resolved reference, so nested arguments beat an enclosing call.
    #[must_use]
    pub fn definition_at(&self, file: FileId, offset: u32) -> Option<Span> {
        let symbol = self
            .references
            .iter()
            .filter(|reference| reference.span.file == file && reference.span.contains(offset))
            .min_by_key(|reference| reference.span.end - reference.span.start)
            .and_then(|reference| {
                self.symbols
                    .iter()
                    .find(|symbol| symbol.id == reference.symbol)
            });
        symbol.and_then(|symbol| symbol.defined_at).or_else(|| {
            self.symbols
                .iter()
                .filter_map(|symbol| symbol.defined_at)
                .find(|span| span.file == file && span.contains(offset))
        })
    }

    pub(crate) fn from_analysis(
        sources: Vec<ProjectSource>,
        analysis: &AnalysisOutput,
        origins: &SourceOriginMap,
    ) -> Self {
        let mut builder = ModelBuilder::new(sources, origins);
        let mut symbols = analysis.symbols.clone();
        for symbol in &mut symbols {
            if let Some(span) = symbol.defined_at {
                let mapped = builder.map(span);
                if let Some(text) = builder.text(mapped) {
                    text.clone_into(&mut symbol.name);
                }
                symbol.defined_at = Some(mapped);
            }
        }
        builder.model.symbols = symbols
            .iter()
            .map(|symbol| ProjectSymbol {
                id: symbol.id,
                name: symbol.name.clone(),
                kind: symbol.kind,
                detail: symbol_detail(symbol, &symbols),
                defined_at: symbol.defined_at,
            })
            .collect();
        builder.model.diagnostics = analysis
            .diagnostics
            .iter()
            .cloned()
            .map(|mut diagnostic| {
                diagnostic.primary.span = builder.map(diagnostic.primary.span);
                for label in &mut diagnostic.secondary {
                    label.span = builder.map(label.span);
                }
                diagnostic
            })
            .collect();
        if !analysis.diagnostics.is_empty() {
            return builder.model;
        }
        for symbol in &symbols {
            if matches!(
                symbol.kind,
                SymbolKind::Constant
                    | SymbolKind::State
                    | SymbolKind::Function
                    | SymbolKind::Task
                    | SymbolKind::Record
                    | SymbolKind::Enum
            ) {
                builder.model.nodes.push(GraphNode {
                    id: symbol_key(symbol.id),
                    label: symbol.name.clone(),
                    kind: format!("{:?}", symbol.kind),
                    span: symbol.defined_at,
                });
            }
        }
        if let Some(ir) = &analysis.ir {
            for global in &ir.globals {
                builder.expression(
                    &global.initializer,
                    &symbol_key(global.symbol),
                    RelationshipKind::Read,
                );
            }
            for routine in &ir.routines {
                let from = if let Some(id) = routine.symbol {
                    symbol_key(id)
                } else {
                    let id = format!("callback:{}", routine.name);
                    builder.model.nodes.push(GraphNode {
                        id: id.clone(),
                        label: routine.name.clone(),
                        kind: "Callback".to_owned(),
                        span: Some(builder.map(routine.span)),
                    });
                    id
                };
                builder.statements(&routine.body, &from);
            }
            for record in &ir.records {
                for field in &record.fields {
                    if let Some(value) = &field.default {
                        builder.expression(
                            value,
                            &symbol_key(record.symbol),
                            RelationshipKind::Read,
                        );
                    }
                }
            }
        }
        builder
            .model
            .nodes
            .sort_by(|left, right| left.id.cmp(&right.id));
        builder.model
    }
}

struct ModelBuilder<'a> {
    model: ProjectModel,
    origins: &'a SourceOriginMap,
    // The linker normalizes CRLF; retain a map back to exact authored byte offsets.
    offsets: BTreeMap<u32, Vec<u32>>,
}

impl<'a> ModelBuilder<'a> {
    fn new(sources: Vec<ProjectSource>, origins: &'a SourceOriginMap) -> Self {
        let offsets = sources
            .iter()
            .map(|source| {
                let bytes = source.text.as_bytes();
                let mut offsets = Vec::with_capacity(bytes.len() + 1);
                for (index, byte) in bytes.iter().enumerate() {
                    if *byte != b'\n' || index == 0 || bytes[index - 1] != b'\r' {
                        offsets.push(u32::try_from(index).unwrap_or(u32::MAX));
                    }
                }
                offsets.push(u32::try_from(bytes.len()).unwrap_or(u32::MAX));
                (source.id.0, offsets)
            })
            .collect();
        Self {
            model: ProjectModel {
                sources,
                ..ProjectModel::default()
            },
            origins,
            offsets,
        }
    }

    fn map(&self, span: Span) -> Span {
        let mut span = self.origins.map_span(span);
        if let Some(offsets) = self.offsets.get(&span.file.0) {
            span.start = *offsets.get(span.start as usize).unwrap_or(&span.start);
            span.end = *offsets.get(span.end as usize).unwrap_or(&span.end);
        }
        span
    }

    fn text(&self, span: Span) -> Option<&str> {
        self.model
            .source(span.file)?
            .text
            .get(span.start as usize..span.end as usize)
    }

    fn reference(&mut self, symbol: SymbolId, span: Span, from: &str, kind: RelationshipKind) {
        let mut span = self.map(span);
        if matches!(kind, RelationshipKind::Call | RelationshipKind::Start)
            && let Some(prefix) = self
                .text(span)
                .and_then(|text| text.split_once('(').map(|pair| pair.0.trim_end()))
        {
            span.end = span.start + u32::try_from(prefix.len()).unwrap_or(0);
        }
        self.model.references.push(SymbolReference { symbol, span });
        let to = symbol_key(symbol);
        if self.model.nodes.iter().any(|node| node.id == to) {
            self.model.edges.push(GraphEdge {
                from: from.to_owned(),
                to,
                kind,
                span,
            });
        }
    }

    fn expression(&mut self, expression: &IrExpression, from: &str, access: RelationshipKind) {
        match &expression.kind {
            IrExpressionKind::Load(symbol) => {
                self.reference(*symbol, expression.span, from, access);
            }
            IrExpressionKind::Call { callee, arguments } => {
                let kind = if access == RelationshipKind::Start {
                    access
                } else {
                    RelationshipKind::Call
                };
                self.reference(*callee, expression.span, from, kind);
                for argument in arguments {
                    self.expression(argument, from, RelationshipKind::Read);
                }
            }
            IrExpressionKind::Asset { name, kind } => {
                let id = format!("asset:{name}");
                let span = self.map(expression.span);
                if !self.model.nodes.iter().any(|node| node.id == id) {
                    self.model.nodes.push(GraphNode {
                        id: id.clone(),
                        label: format!("#{name}"),
                        kind: kind.type_name().to_owned(),
                        span: None,
                    });
                }
                self.model.edges.push(GraphEdge {
                    from: from.to_owned(),
                    to: id,
                    kind: RelationshipKind::Asset,
                    span,
                });
            }
            IrExpressionKind::Array(items) => {
                for item in items {
                    self.expression(item, from, RelationshipKind::Read);
                }
            }
            IrExpressionKind::Unary { operand, .. } => {
                self.expression(operand, from, RelationshipKind::Read);
            }
            IrExpressionKind::Binary { left, right, .. } => {
                self.expression(left, from, RelationshipKind::Read);
                self.expression(right, from, RelationshipKind::Read);
            }
            IrExpressionKind::Field { subject, .. } => self.expression(subject, from, access),
            IrExpressionKind::Index { subject, index } => {
                self.expression(subject, from, access);
                self.expression(index, from, RelationshipKind::Read);
            }
            IrExpressionKind::Literal(_) | IrExpressionKind::Error => {}
        }
    }

    fn place(&mut self, place: &IrPlace, span: Span, from: &str) {
        match place {
            IrPlace::Symbol(symbol) => self.reference(*symbol, span, from, RelationshipKind::Write),
            IrPlace::Field { subject, .. } => {
                self.expression(subject, from, RelationshipKind::Write);
            }
            IrPlace::Index { subject, index } => {
                self.expression(subject, from, RelationshipKind::Write);
                self.expression(index, from, RelationshipKind::Read);
            }
            IrPlace::Error => {}
        }
    }

    fn statements(&mut self, statements: &[IrStatement], from: &str) {
        for statement in statements {
            match &statement.kind {
                IrStatementKind::Let { value, .. }
                | IrStatementKind::Expression(value)
                | IrStatementKind::Wait(value) => {
                    self.expression(value, from, RelationshipKind::Read);
                }
                IrStatementKind::Store { target, value, .. } => {
                    self.place(target, statement.span, from);
                    self.expression(value, from, RelationshipKind::Read);
                }
                IrStatementKind::Return(value) => {
                    if let Some(value) = value {
                        self.expression(value, from, RelationshipKind::Read);
                    }
                }
                IrStatementKind::Start(value) => {
                    self.expression(value, from, RelationshipKind::Start);
                }
                IrStatementKind::Assert { condition, message } => {
                    self.expression(condition, from, RelationshipKind::Read);
                    if let Some(value) = message {
                        self.expression(value, from, RelationshipKind::Read);
                    }
                }
                IrStatementKind::If {
                    branches,
                    else_body,
                } => {
                    for branch in branches {
                        self.expression(&branch.condition, from, RelationshipKind::Read);
                        self.statements(&branch.body, from);
                    }
                    if let Some(body) = else_body {
                        self.statements(body, from);
                    }
                }
                IrStatementKind::While { condition, body } => {
                    self.expression(condition, from, RelationshipKind::Read);
                    self.statements(body, from);
                }
                IrStatementKind::For { iterable, body, .. } => {
                    self.expression(iterable, from, RelationshipKind::Read);
                    self.statements(body, from);
                }
                IrStatementKind::Match { subject, arms } => {
                    self.expression(subject, from, RelationshipKind::Read);
                    for arm in arms {
                        self.statements(&arm.body, from);
                    }
                }
                IrStatementKind::Break | IrStatementKind::Continue => {}
            }
        }
    }
}

fn symbol_key(symbol: SymbolId) -> String {
    format!("symbol:{}", symbol.0)
}

fn symbol_detail(symbol: &Symbol, symbols: &[Symbol]) -> String {
    if let Type::Function(function) = &symbol.r#type {
        format!(
            "{} {}({}) -> {}",
            if function.task { "task" } else { "fn" },
            symbol.name,
            function
                .parameters
                .iter()
                .map(|ty| type_label(ty, symbols))
                .collect::<Vec<_>>()
                .join(", "),
            type_label(&function.return_type, symbols)
        )
    } else {
        format!("{}: {}", symbol.name, type_label(&symbol.r#type, symbols))
    }
}

fn type_label(ty: &Type, symbols: &[Symbol]) -> String {
    match ty {
        Type::Asset(kind) => kind.type_name().to_owned(),
        Type::Option(inner) => format!("Option[{}]", type_label(inner, symbols)),
        Type::Array { element, length } => format!("[{}, {length}]", type_label(element, symbols)),
        Type::List { element, capacity } => {
            format!("List[{}, {capacity}]", type_label(element, symbols))
        }
        Type::Record(id) | Type::Enum(id) => symbols
            .iter()
            .find(|symbol| symbol.id == *id)
            .map_or_else(|| "?".to_owned(), |symbol| symbol.name.clone()),
        Type::Function(signature) => format!(
            "fn({}) -> {}",
            signature
                .parameters
                .iter()
                .map(|ty| type_label(ty, symbols))
                .collect::<Vec<_>>()
                .join(", "),
            type_label(&signature.return_type, symbols)
        ),
        Type::Unknown => "T".to_owned(),
        _ => format!("{ty:?}"),
    }
}

/// Parses source modules without linking, preserving actionable syntax spans during incomplete edits.
pub(crate) fn source_model(
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<ProjectModel, crate::CartridgeError> {
    let mut model = ProjectModel::default();
    for (index, (path, bytes)) in files
        .iter()
        .filter(|(path, _)| {
            std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension == "modl")
        })
        .enumerate()
    {
        let text = std::str::from_utf8(bytes).map_err(|_| crate::CartridgeError {
            code: "M014003",
            message: format!("module '{path}' is not UTF-8"),
        })?;
        let id = FileId(u32::try_from(index).unwrap_or(u32::MAX));
        model
            .diagnostics
            .extend(crate::parse(&SourceFile::new(id, path, text)).diagnostics);
        model.sources.push(ProjectSource {
            id,
            path: path.clone(),
            text: text.to_owned(),
        });
    }
    Ok(model)
}
