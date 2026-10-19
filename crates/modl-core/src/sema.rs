use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

use serde::{Deserialize, Serialize};

use crate::{
    ast::{
        Assertion, BinaryOperator, Block, Callback, CallbackKind, Enum, Expression, ExpressionKind,
        Function, Item, Literal, MatchStatement, Module, Name, Pattern, PatternKind, Record, State,
        Statement, StatementKind, Task, TypeArgument, TypeKind, TypeNode, UnaryOperator,
    },
    diagnostic::Diagnostic,
    ir::{
        IrBranch, IrEnum, IrExpression, IrExpressionKind, IrField, IrGlobal, IrMatchArm, IrModule,
        IrPattern, IrPlace, IrRecord, IrRoutine, IrStatement, IrStatementKind, IrVariant,
        RoutineKind, Symbol, SymbolKind,
    },
    parser::parse,
    span::{SourceFile, Span},
    token::Token,
    types::{AssetCatalog, AssetKind, FunctionType, SymbolId, Type},
};

/// Complete front-end result for compiler-explorer, CLI, and language-service consumers.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AnalysisOutput {
    pub module: Module,
    pub tokens: Vec<Token>,
    pub symbols: Vec<Symbol>,
    pub ir: Option<IrModule>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Compiler-derived standard API metadata shared by Studio and external editors.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BuiltinDefinition {
    pub name: String,
    pub signature: String,
    pub callable: bool,
}

/// Returns the complete current API from the same registrations used by type checking.
#[must_use]
pub fn builtin_definitions() -> &'static [BuiltinDefinition] {
    static DEFINITIONS: OnceLock<Vec<BuiltinDefinition>> = OnceLock::new();
    DEFINITIONS.get_or_init(|| {
        let source = SourceFile::new(crate::FileId(0), "builtins.modl", "");
        let parsed = parse(&source);
        let assets = AssetCatalog::default();
        let mut analyzer = Analyzer::new(&parsed.module, &assets);
        analyzer.install_builtins();
        let mut definitions = analyzer
            .symbols
            .iter()
            .map(|symbol| {
                let (signature, callable) = if let Type::Function(function) = &symbol.r#type {
                    (
                        format!(
                            "fn {}({}) -> {}",
                            symbol.name,
                            function
                                .parameters
                                .iter()
                                .map(|parameter| analyzer.describe_type(parameter))
                                .collect::<Vec<_>>()
                                .join(", "),
                            analyzer.describe_type(&function.return_type)
                        ),
                        true,
                    )
                } else {
                    (
                        format!(
                            "{}: {}",
                            symbol.name,
                            analyzer.describe_type(&symbol.r#type)
                        ),
                        false,
                    )
                };
                BuiltinDefinition {
                    name: symbol.name.clone(),
                    signature,
                    callable,
                }
            })
            .collect::<Vec<_>>();
        definitions.sort_by(|left, right| left.name.cmp(&right.name));
        definitions
    })
}

/// Parses, resolves, type-checks, and lowers one MODL module to typed IR.
#[must_use]
pub fn analyze_module(source: &SourceFile, assets: &AssetCatalog) -> AnalysisOutput {
    let parsed = parse(source);
    if !parsed.diagnostics.is_empty() {
        return AnalysisOutput {
            module: parsed.module,
            tokens: parsed.tokens,
            symbols: Vec::new(),
            ir: None,
            diagnostics: parsed.diagnostics,
        };
    }
    let (symbols, ir, diagnostics) = Analyzer::new(&parsed.module, assets).run();
    AnalysisOutput {
        module: parsed.module,
        tokens: parsed.tokens,
        symbols,
        ir: Some(ir),
        diagnostics,
    }
}

#[derive(Clone, Debug)]
struct RecordFieldInfo {
    name: String,
    r#type: Type,
    has_default: bool,
    span: Span,
}

#[derive(Clone, Debug)]
struct VariantInfo {
    symbol: SymbolId,
    owner: SymbolId,
    name: String,
    fields: Vec<Type>,
    span: Span,
}

#[derive(Clone, Debug)]
enum ConstantValue {
    Int(i64),
    Num(f64),
    Bool(bool),
    Text(String),
    None,
}

struct Analyzer<'syntax> {
    module: &'syntax Module,
    assets: &'syntax AssetCatalog,
    diagnostics: Vec<Diagnostic>,
    symbols: Vec<Symbol>,
    globals: BTreeMap<String, SymbolId>,
    type_names: BTreeMap<String, Type>,
    definition_ids: BTreeMap<u32, SymbolId>,
    record_fields: BTreeMap<SymbolId, Vec<RecordFieldInfo>>,
    enum_variants: BTreeMap<SymbolId, Vec<VariantInfo>>,
    scopes: Vec<BTreeMap<String, SymbolId>>,
    current_return_type: Type,
    current_routine: Option<RoutineKind>,
    loop_depth: u32,
    callbacks: BTreeMap<CallbackKind, Span>,
    constant_values: BTreeMap<SymbolId, ConstantValue>,
    allowed_task_call_start: Option<u32>,
}

impl<'syntax> Analyzer<'syntax> {
    fn new(module: &'syntax Module, assets: &'syntax AssetCatalog) -> Self {
        Self {
            module,
            assets,
            diagnostics: Vec::new(),
            symbols: Vec::new(),
            globals: BTreeMap::new(),
            type_names: BTreeMap::new(),
            definition_ids: BTreeMap::new(),
            record_fields: BTreeMap::new(),
            enum_variants: BTreeMap::new(),
            scopes: Vec::new(),
            current_return_type: Type::Unit,
            current_routine: None,
            loop_depth: 0,
            callbacks: BTreeMap::new(),
            constant_values: BTreeMap::new(),
            allowed_task_call_start: None,
        }
    }

    fn run(mut self) -> (Vec<Symbol>, IrModule, Vec<Diagnostic>) {
        self.install_builtins();
        self.collect_nominal_types();
        self.collect_nominal_members();
        self.collect_value_declarations();

        let records = self.lower_records();
        let enums = self.lower_enums();
        let mut ir = IrModule {
            globals: Vec::new(),
            records,
            enums,
            routines: Vec::new(),
        };
        for item in &self.module.items {
            match item {
                Item::Constant(constant) => {
                    if let Some(global) =
                        self.check_global(constant.name.span, &constant.value, constant.span, false)
                    {
                        ir.globals.push(global);
                    }
                }
                Item::State(state) => {
                    if let Some(global) = self.check_state(state) {
                        ir.globals.push(global);
                    }
                }
                Item::Function(function) => {
                    if let Some(routine) = self.check_function(function) {
                        ir.routines.push(routine);
                    }
                }
                Item::Task(task) => {
                    if let Some(routine) = self.check_task(task) {
                        ir.routines.push(routine);
                    }
                }
                Item::Callback(callback) => {
                    ir.routines.push(self.check_callback(callback));
                }
                Item::Assertion(assertion) => self.check_module_assertion(assertion),
                Item::Import(_) | Item::Record(_) | Item::Enum(_) => {}
            }
        }
        (self.symbols, ir, self.diagnostics)
    }

    fn install_builtins(&mut self) {
        self.install_shape_builtins();
        self.install_display_builtins();
        self.install_runtime_builtins();
        self.install_builtin_values();
        self.install_math_builtins();
        self.install_text_builtins();
        self.builtin("bit_not", vec![Type::Int], Type::Int);
        for name in [
            "bit_and", "bit_or", "bit_xor", "bit_shl", "bit_shr", "bit_ushr",
        ] {
            self.builtin(name, vec![Type::Int, Type::Int], Type::Int);
        }
        self.install_memory_builtins();
    }

    fn install_text_builtins(&mut self) {
        self.builtin(
            "format_int",
            vec![Type::Int, Type::Int, Type::Int],
            Type::Text,
        );
        self.builtin(
            "format_num",
            vec![Type::Num, Type::Int, Type::Int],
            Type::Text,
        );
        self.builtin(
            "parse_int",
            vec![Type::Text, Type::Int],
            Type::Option(Box::new(Type::Int)),
        );
        self.builtin(
            "parse_num",
            vec![Type::Text],
            Type::Option(Box::new(Type::Num)),
        );
        self.builtin("text_length", vec![Type::Text], Type::Int);
        self.builtin(
            "text_slice",
            vec![Type::Text, Type::Int, Type::Int],
            Type::Text,
        );
        self.builtin(
            "text_code",
            vec![Type::Text, Type::Int],
            Type::Option(Box::new(Type::Int)),
        );
        self.builtin(
            "text_char",
            vec![Type::Int],
            Type::Option(Box::new(Type::Text)),
        );
        self.builtin(
            "text_pad",
            vec![Type::Text, Type::Int, Type::Text],
            Type::Text,
        );
        self.builtin(
            "text_split",
            vec![Type::Text, Type::Text],
            Type::List {
                element: Box::new(Type::Text),
                capacity: 256,
            },
        );
        self.builtin("list_length", vec![Type::Unknown], Type::Int);
    }

    fn install_math_builtins(&mut self) {
        for (name, count) in [("abs", 1), ("min", 2), ("max", 2), ("clamp", 3)] {
            self.builtin(name, vec![Type::Unknown; count], Type::Unknown);
        }
        for name in ["floor", "ceil", "round", "trunc"] {
            self.builtin(name, vec![Type::Num], Type::Int);
        }
        for name in ["sqrt", "sin", "cos"] {
            self.builtin(name, vec![Type::Num], Type::Num);
        }
        self.builtin("atan2", vec![Type::Num, Type::Num], Type::Num);
    }

    fn install_shape_builtins(&mut self) {
        self.builtin("clear", vec![Type::Color], Type::Unit);
        self.builtin("pixel", vec![Type::Int, Type::Int, Type::Color], Type::Unit);
        self.builtin(
            "line",
            vec![Type::Int, Type::Int, Type::Int, Type::Int, Type::Color],
            Type::Unit,
        );
        for name in ["rect", "rect_fill"] {
            self.builtin(
                name,
                vec![Type::Int, Type::Int, Type::Int, Type::Int, Type::Color],
                Type::Unit,
            );
        }
        for name in ["circle", "circle_fill"] {
            self.builtin(
                name,
                vec![Type::Int, Type::Int, Type::Int, Type::Color],
                Type::Unit,
            );
        }
        self.builtin(
            "triangle",
            vec![
                Type::Int,
                Type::Int,
                Type::Int,
                Type::Int,
                Type::Int,
                Type::Int,
                Type::Color,
            ],
            Type::Unit,
        );
    }

    fn install_display_builtins(&mut self) {
        self.builtin(
            "sprite",
            vec![Type::Asset(AssetKind::Sprite), Type::Int, Type::Int],
            Type::Unit,
        );
        self.builtin(
            "sprite_xform",
            vec![
                Type::Asset(AssetKind::Sprite),
                Type::Int,
                Type::Int,
                Type::Int,
                Type::Int,
                Type::Bool,
                Type::Bool,
            ],
            Type::Unit,
        );
        self.builtin(
            "animation",
            vec![
                Type::Asset(AssetKind::Animation),
                Type::Int,
                Type::Int,
                Type::Int,
            ],
            Type::Unit,
        );
        self.builtin(
            "map",
            vec![Type::Asset(AssetKind::Map), Type::Int, Type::Int],
            Type::Unit,
        );
        self.builtin(
            "map_cell",
            vec![Type::Asset(AssetKind::Map), Type::Int, Type::Int, Type::Int],
            Type::Int,
        );
        self.builtin(
            "map_flag",
            vec![
                Type::Asset(AssetKind::Map),
                Type::Int,
                Type::Int,
                Type::Int,
                Type::Int,
            ],
            Type::Bool,
        );
        self.builtin("camera", vec![Type::Int, Type::Int], Type::Unit);
        self.builtin(
            "clip",
            vec![Type::Int, Type::Int, Type::Int, Type::Int],
            Type::Unit,
        );
        self.builtin("clip_reset", Vec::new(), Type::Unit);
        self.builtin("pal", vec![Type::Color, Type::Color], Type::Unit);
        self.builtin("pal_reset", Vec::new(), Type::Unit);
        self.builtin("palt", vec![Type::Color, Type::Bool], Type::Unit);
        self.builtin("palt_reset", Vec::new(), Type::Unit);
        self.builtin("raster_scroll", vec![Type::Int, Type::Int], Type::Unit);
        self.builtin(
            "dither",
            vec![Type::Int, Type::Int, Type::Color, Type::Color, Type::Int],
            Type::Color,
        );
        self.builtin(
            "print",
            vec![Type::Text, Type::Int, Type::Int, Type::Color],
            Type::Unit,
        );
    }

    fn install_runtime_builtins(&mut self) {
        self.builtin("btn", vec![Type::Controller, Type::Button], Type::Bool);
        self.builtin("btnp", vec![Type::Controller, Type::Button], Type::Bool);
        self.builtin("pointer_x", Vec::new(), Type::Int);
        self.builtin("pointer_y", Vec::new(), Type::Int);
        self.builtin("pointer_inside", Vec::new(), Type::Bool);
        self.builtin("pointer_primary", Vec::new(), Type::Bool);
        self.builtin("pointer_secondary", Vec::new(), Type::Bool);
        self.builtin("rng_int", vec![Type::Int, Type::Int], Type::Int);
        self.builtin("rng_num", Vec::new(), Type::Num);
        self.builtin("save_get_int", vec![Type::Text, Type::Int], Type::Int);
        self.builtin("save_set_int", vec![Type::Text, Type::Int], Type::Unit);
        self.builtin("sfx", vec![Type::Asset(AssetKind::Sound)], Type::Unit);
        self.builtin("music", vec![Type::Asset(AssetKind::Music)], Type::Unit);
        self.builtin("music_stop", Vec::new(), Type::Unit);
        self.builtin("some", vec![Type::Unknown], Type::Unknown);
        self.builtin("is_some", vec![Type::Unknown], Type::Bool);
        self.builtin(
            "unwrap_or",
            vec![Type::Unknown, Type::Unknown],
            Type::Unknown,
        );
        self.builtin("Vec2", vec![Type::Num, Type::Num], Type::Vec2);
        self.builtin(
            "Rect",
            vec![Type::Num, Type::Num, Type::Num, Type::Num],
            Type::Rect,
        );
    }

    fn install_builtin_values(&mut self) {
        for (name, r#type) in [
            ("pad1", Type::Controller),
            ("pad2", Type::Controller),
            ("pad3", Type::Controller),
            ("pad4", Type::Controller),
            ("up", Type::Button),
            ("down", Type::Button),
            ("left", Type::Button),
            ("right", Type::Button),
            ("a", Type::Button),
            ("b", Type::Button),
            ("x", Type::Button),
            ("y", Type::Button),
            ("l", Type::Button),
            ("r", Type::Button),
            ("start_button", Type::Button),
            ("menu", Type::Button),
        ] {
            self.builtin_value(name, r#type);
        }
    }

    fn builtin(&mut self, name: &'static str, parameters: Vec<Type>, return_type: Type) {
        let r#type = Type::Function(FunctionType {
            required_parameters: u32::try_from(parameters.len()).unwrap_or(u32::MAX),
            parameters,
            return_type: Box::new(return_type),
            task: false,
        });
        let id = self.add_symbol(name, SymbolKind::Builtin, r#type, false, None);
        self.globals.insert(name.to_owned(), id);
    }

    fn builtin_value(&mut self, name: &'static str, r#type: Type) {
        let id = self.add_symbol(name, SymbolKind::Builtin, r#type, false, None);
        self.globals.insert(name.to_owned(), id);
    }

    fn collect_nominal_types(&mut self) {
        for item in &self.module.items {
            let (name, kind) = match item {
                Item::Record(record) => (&record.name, SymbolKind::Record),
                Item::Enum(enumeration) => (&enumeration.name, SymbolKind::Enum),
                _ => continue,
            };
            if let Some(id) = self.declare_global(name, kind, Type::Unknown, false) {
                let nominal = if kind == SymbolKind::Record {
                    Type::Record(id)
                } else {
                    Type::Enum(id)
                };
                self.type_names.insert(name.value.clone(), nominal);
                self.definition_ids.insert(name.span.start, id);
            }
        }
    }

    fn collect_nominal_members(&mut self) {
        for item in &self.module.items {
            match item {
                Item::Record(record) => self.collect_record(record),
                Item::Enum(enumeration) => self.collect_enum(enumeration),
                _ => {}
            }
        }
    }

    fn collect_record(&mut self, record: &Record) {
        let Some(symbol) = self.definition_ids.get(&record.name.span.start).copied() else {
            return;
        };
        let mut seen = BTreeMap::<String, Span>::new();
        let mut fields = Vec::new();
        let mut saw_default = false;
        for field in &record.fields {
            if let Some(first) = seen.get(&field.name.value) {
                self.duplicate(
                    field.name.span,
                    *first,
                    format!("record field `{}` is declared twice", field.name.value),
                );
                continue;
            }
            seen.insert(field.name.value.clone(), field.name.span);
            if field.default.is_none() && saw_default {
                self.diagnostics.push(
                    Diagnostic::error(
                        "M013006",
                        field.span,
                        "required record fields cannot follow fields with defaults",
                    )
                    .with_primary_label("move this field before defaulted fields"),
                );
            }
            saw_default |= field.default.is_some();
            fields.push(RecordFieldInfo {
                name: field.name.value.clone(),
                r#type: self.resolve_type(&field.type_annotation),
                has_default: field.default.is_some(),
                span: field.span,
            });
        }
        let constructor = Type::Function(FunctionType {
            required_parameters: u32::try_from(
                fields.iter().filter(|field| !field.has_default).count(),
            )
            .unwrap_or(u32::MAX),
            parameters: fields.iter().map(|field| field.r#type.clone()).collect(),
            return_type: Box::new(Type::Record(symbol)),
            task: false,
        });
        self.symbol_mut(symbol).r#type = constructor;
        self.record_fields.insert(symbol, fields);
    }

    fn collect_enum(&mut self, enumeration: &Enum) {
        let Some(owner) = self
            .definition_ids
            .get(&enumeration.name.span.start)
            .copied()
        else {
            return;
        };
        self.symbol_mut(owner).r#type = Type::Module;
        let mut seen = BTreeMap::<String, Span>::new();
        let mut variants = Vec::new();
        for variant in &enumeration.variants {
            if let Some(first) = seen.get(&variant.name.value) {
                self.duplicate(
                    variant.name.span,
                    *first,
                    format!("enum variant `{}` is declared twice", variant.name.value),
                );
                continue;
            }
            seen.insert(variant.name.value.clone(), variant.name.span);
            let fields: Vec<_> = variant
                .fields
                .iter()
                .map(|field| self.resolve_type(field))
                .collect();
            let r#type = if fields.is_empty() {
                Type::Enum(owner)
            } else {
                Type::Function(FunctionType {
                    required_parameters: u32::try_from(fields.len()).unwrap_or(u32::MAX),
                    parameters: fields.clone(),
                    return_type: Box::new(Type::Enum(owner)),
                    task: false,
                })
            };
            let qualified = format!("{}.{}", enumeration.name.value, variant.name.value);
            let symbol = self.add_symbol(
                qualified,
                SymbolKind::Variant,
                r#type,
                false,
                Some(variant.name.span),
            );
            variants.push(VariantInfo {
                symbol,
                owner,
                name: variant.name.value.clone(),
                fields,
                span: variant.span,
            });
        }
        self.enum_variants.insert(owner, variants);
    }

    fn collect_value_declarations(&mut self) {
        for item in &self.module.items {
            match item {
                Item::Import(import) => {
                    let Some(name) = import.alias.as_ref().or_else(|| import.path.last()) else {
                        continue;
                    };
                    if let Some(id) =
                        self.declare_global(name, SymbolKind::Import, Type::Module, false)
                    {
                        self.definition_ids.insert(name.span.start, id);
                    }
                }
                Item::Constant(constant) => {
                    let r#type = constant
                        .type_annotation
                        .as_ref()
                        .map_or(Type::Unknown, |node| self.resolve_type(node));
                    if let Some(id) =
                        self.declare_global(&constant.name, SymbolKind::Constant, r#type, false)
                    {
                        self.definition_ids.insert(constant.name.span.start, id);
                    }
                }
                Item::State(state) => {
                    let r#type = self.resolve_type(&state.type_annotation);
                    if let Some(id) =
                        self.declare_global(&state.name, SymbolKind::State, r#type, true)
                    {
                        self.definition_ids.insert(state.name.span.start, id);
                    }
                }
                Item::Function(function) => {
                    let parameters: Vec<Type> = function
                        .parameters
                        .iter()
                        .map(|parameter| self.resolve_type(&parameter.type_annotation))
                        .collect();
                    let return_type = self.resolve_type(&function.return_type);
                    let r#type = Type::Function(FunctionType {
                        required_parameters: u32::try_from(parameters.len()).unwrap_or(u32::MAX),
                        parameters,
                        return_type: Box::new(return_type),
                        task: false,
                    });
                    if let Some(id) =
                        self.declare_global(&function.name, SymbolKind::Function, r#type, false)
                    {
                        self.definition_ids.insert(function.name.span.start, id);
                    }
                }
                Item::Task(task) => {
                    let parameters: Vec<Type> = task
                        .parameters
                        .iter()
                        .map(|parameter| self.resolve_type(&parameter.type_annotation))
                        .collect();
                    let r#type = Type::Function(FunctionType {
                        required_parameters: u32::try_from(parameters.len()).unwrap_or(u32::MAX),
                        parameters,
                        return_type: Box::new(Type::Unit),
                        task: true,
                    });
                    if let Some(id) =
                        self.declare_global(&task.name, SymbolKind::Task, r#type, false)
                    {
                        self.definition_ids.insert(task.name.span.start, id);
                    }
                }
                Item::Callback(_) | Item::Record(_) | Item::Enum(_) | Item::Assertion(_) => {}
            }
        }
    }

    fn lower_records(&mut self) -> Vec<IrRecord> {
        let records: Vec<_> = self
            .module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Record(record) => Some(record.clone()),
                _ => None,
            })
            .collect();
        let mut lowered = Vec::new();
        for record in records {
            let Some(symbol) = self.definition_ids.get(&record.name.span.start).copied() else {
                continue;
            };
            let fields = self.record_fields.get(&symbol).cloned().unwrap_or_default();
            let fields = record
                .fields
                .iter()
                .zip(fields)
                .map(|(syntax, field)| IrField {
                    name: field.name,
                    default: syntax
                        .default
                        .as_ref()
                        .map(|default| self.check_expression(default, Some(&field.r#type))),
                    r#type: field.r#type,
                    span: field.span,
                })
                .collect();
            lowered.push(IrRecord { symbol, fields });
        }
        lowered
    }

    fn lower_enums(&self) -> Vec<IrEnum> {
        self.enum_variants
            .iter()
            .map(|(symbol, variants)| IrEnum {
                symbol: *symbol,
                variants: variants
                    .iter()
                    .map(|variant| IrVariant {
                        symbol: variant.symbol,
                        fields: variant.fields.clone(),
                        span: variant.span,
                    })
                    .collect(),
            })
            .collect()
    }

    fn check_global(
        &mut self,
        definition: Span,
        value: &Expression,
        span: Span,
        mutable: bool,
    ) -> Option<IrGlobal> {
        let symbol = self.definition_ids.get(&definition.start).copied()?;
        let declared = self.symbol(symbol).r#type.clone();
        let expected = (!matches!(declared, Type::Unknown)).then_some(&declared);
        let initializer = self.check_expression(value, expected);
        if matches!(declared, Type::Unknown) {
            self.symbol_mut(symbol).r#type = initializer.r#type.clone();
        }
        if !mutable && self.symbol(symbol).kind == SymbolKind::Constant {
            if let Some(constant) = self.evaluate_constant(value) {
                self.constant_values.insert(symbol, constant);
            } else {
                self.diagnostics.push(
                    Diagnostic::error(
                        "M013120",
                        value.span,
                        "constant initializer is not a compile-time expression",
                    )
                    .with_primary_label(
                        "constants cannot depend on mutable state or runtime calls",
                    ),
                );
            }
        }
        Some(IrGlobal {
            symbol,
            mutable,
            initializer,
            span,
        })
    }

    fn check_state(&mut self, state: &State) -> Option<IrGlobal> {
        self.check_global(state.name.span, &state.value, state.span, true)
    }

    fn check_function(&mut self, function: &Function) -> Option<IrRoutine> {
        let symbol = self
            .definition_ids
            .get(&function.name.span.start)
            .copied()?;
        let Type::Function(signature) = self.symbol(symbol).r#type.clone() else {
            return None;
        };
        let routine = self.check_routine(
            Some(symbol),
            function.name.value.clone(),
            RoutineKind::Function,
            &function.parameters,
            &signature.parameters,
            (*signature.return_type).clone(),
            &function.body,
            function.span,
        );
        if routine.return_type != Type::Unit && !block_guarantees_return(&function.body) {
            self.diagnostics.push(
                Diagnostic::error(
                    "M013109",
                    function.name.span,
                    format!(
                        "function `{}` can reach its end without returning {}",
                        function.name.value,
                        self.describe_type(&routine.return_type)
                    ),
                )
                .with_primary_label("not every control-flow path returns a value"),
            );
        }
        Some(routine)
    }

    fn check_task(&mut self, task: &Task) -> Option<IrRoutine> {
        let symbol = self.definition_ids.get(&task.name.span.start).copied()?;
        let Type::Function(signature) = self.symbol(symbol).r#type.clone() else {
            return None;
        };
        Some(self.check_routine(
            Some(symbol),
            task.name.value.clone(),
            RoutineKind::Task,
            &task.parameters,
            &signature.parameters,
            Type::Unit,
            &task.body,
            task.span,
        ))
    }

    fn check_callback(&mut self, callback: &Callback) -> IrRoutine {
        if let Some(first) = self.callbacks.insert(callback.kind, callback.span) {
            self.duplicate(
                callback.span,
                first,
                format!("`on {:?}` is declared more than once", callback.kind).to_lowercase(),
            );
        }
        let expected = if callback.kind == CallbackKind::Raster {
            vec![Type::Int]
        } else {
            Vec::new()
        };
        if callback.parameters.len() != expected.len() {
            self.diagnostics.push(
                Diagnostic::error(
                    "M013005",
                    callback.span,
                    format!(
                        "`on {:?}` expects {} parameter(s)",
                        callback.kind,
                        expected.len()
                    )
                    .to_lowercase(),
                )
                .with_primary_label("callback signature does not match the system callback"),
            );
        }
        self.check_routine(
            None,
            format!("@{:?}", callback.kind).to_lowercase(),
            RoutineKind::Callback(callback.kind),
            &callback.parameters,
            &expected,
            Type::Unit,
            &callback.body,
            callback.span,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn check_routine(
        &mut self,
        symbol: Option<SymbolId>,
        name: String,
        kind: RoutineKind,
        parameters: &[crate::ast::Parameter],
        parameter_types: &[Type],
        return_type: Type,
        body: &Block,
        span: Span,
    ) -> IrRoutine {
        let prior_return = std::mem::replace(&mut self.current_return_type, return_type.clone());
        let prior_routine = self.current_routine.replace(kind);
        let prior_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        self.scopes.push(BTreeMap::new());
        let mut parameter_symbols = Vec::new();
        for (index, parameter) in parameters.iter().enumerate() {
            let r#type = parameter_types.get(index).cloned().unwrap_or(Type::Error);
            parameter_symbols.push(self.declare_local(
                &parameter.name,
                SymbolKind::Parameter,
                r#type,
                false,
            ));
        }
        let body = self.check_statements(body);
        self.scopes.pop();
        self.current_return_type = prior_return;
        self.current_routine = prior_routine;
        self.loop_depth = prior_loop_depth;
        IrRoutine {
            symbol,
            name,
            kind,
            parameters: parameter_symbols,
            return_type,
            body,
            span,
        }
    }

    fn check_statements(&mut self, statements: &Block) -> Vec<IrStatement> {
        statements
            .iter()
            .map(|statement| self.check_statement(statement))
            .collect()
    }

    fn check_nested_block(&mut self, statements: &Block) -> Vec<IrStatement> {
        self.scopes.push(BTreeMap::new());
        let checked = self.check_statements(statements);
        self.scopes.pop();
        checked
    }

    #[allow(clippy::too_many_lines)]
    fn check_statement(&mut self, statement: &Statement) -> IrStatement {
        let kind = match &statement.kind {
            StatementKind::Let {
                mutable,
                name,
                value,
            } => {
                let value = self.check_expression(value, None);
                let symbol =
                    self.declare_local(name, SymbolKind::Local, value.r#type.clone(), *mutable);
                IrStatementKind::Let {
                    symbol,
                    mutable: *mutable,
                    value,
                }
            }
            StatementKind::Assignment {
                target,
                operator,
                value,
            } => {
                let (place, target_type) = self.lower_place(target);
                let value = self.check_expression(value, Some(&target_type));
                if *operator != crate::ast::AssignmentOperator::Assign
                    && (!target_type.is_numeric() || !value.r#type.is_numeric())
                {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "M013102",
                            statement.span,
                            "compound assignment requires numeric operands",
                        )
                        .with_primary_label("operator is not defined for these types"),
                    );
                }
                IrStatementKind::Store {
                    target: place,
                    operator: *operator,
                    value,
                }
            }
            StatementKind::Expression(expression) => {
                IrStatementKind::Expression(self.check_expression(expression, None))
            }
            StatementKind::Return(value) => {
                let expected = self.current_return_type.clone();
                let value = value
                    .as_ref()
                    .map(|expression| self.check_expression(expression, Some(&expected)));
                if value.is_none() && expected != Type::Unit {
                    self.type_mismatch(statement.span, &Type::Unit, &expected, "return value");
                }
                IrStatementKind::Return(value)
            }
            StatementKind::Break | StatementKind::Continue => {
                if self.loop_depth == 0 {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "M013117",
                            statement.span,
                            "loop control can only be used inside a loop",
                        )
                        .with_primary_label("no enclosing `for` or `while` loop"),
                    );
                }
                if matches!(statement.kind, StatementKind::Break) {
                    IrStatementKind::Break
                } else {
                    IrStatementKind::Continue
                }
            }
            StatementKind::Wait(duration) => {
                if self.current_routine != Some(RoutineKind::Task) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "M013110",
                            statement.span,
                            "`wait` is available only inside a task",
                        )
                        .with_primary_label("ordinary functions and callbacks cannot suspend"),
                    );
                }
                IrStatementKind::Wait(self.check_expression(duration, Some(&Type::Duration)))
            }
            StatementKind::Start(expression) => {
                self.allowed_task_call_start = Some(expression.span.start);
                let expression = self.check_expression(expression, None);
                self.allowed_task_call_start = None;
                let valid = match expression.kind {
                    IrExpressionKind::Call { callee, .. } => {
                        self.symbol(callee).kind == SymbolKind::Task
                    }
                    _ => false,
                };
                if !valid {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "M013111",
                            statement.span,
                            "`start` requires a direct task call",
                        )
                        .with_primary_label("this expression does not name a task"),
                    );
                }
                IrStatementKind::Start(expression)
            }
            StatementKind::Assert(assertion) => {
                let condition = self.check_expression(&assertion.condition, Some(&Type::Bool));
                let message = assertion
                    .message
                    .as_ref()
                    .map(|value| self.check_expression(value, Some(&Type::Text)));
                IrStatementKind::Assert { condition, message }
            }
            StatementKind::If(statement) => {
                let branches = statement
                    .branches
                    .iter()
                    .map(|branch| IrBranch {
                        condition: self.check_expression(&branch.condition, Some(&Type::Bool)),
                        body: self.check_nested_block(&branch.body),
                        span: branch.span,
                    })
                    .collect();
                let else_body = statement
                    .else_body
                    .as_ref()
                    .map(|body| self.check_nested_block(body));
                IrStatementKind::If {
                    branches,
                    else_body,
                }
            }
            StatementKind::While { condition, body } => {
                let condition = self.check_expression(condition, Some(&Type::Bool));
                self.loop_depth += 1;
                let body = self.check_nested_block(body);
                self.loop_depth -= 1;
                IrStatementKind::While { condition, body }
            }
            StatementKind::For {
                binding,
                iterable,
                body,
            } => {
                let iterable = self.check_expression(iterable, None);
                let element_type = match &iterable.r#type {
                    Type::Range => Type::Int,
                    Type::Array { element, .. } | Type::List { element, .. } => (**element).clone(),
                    Type::Error => Type::Error,
                    other => {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "M013102",
                                iterable.span,
                                format!("{} is not iterable", self.describe_type(other)),
                            )
                            .with_primary_label(
                                "expected a range, fixed array, or fixed-capacity list",
                            ),
                        );
                        Type::Error
                    }
                };
                self.scopes.push(BTreeMap::new());
                let symbol = self.declare_local(binding, SymbolKind::Local, element_type, false);
                self.loop_depth += 1;
                let body = self.check_statements(body);
                self.loop_depth -= 1;
                self.scopes.pop();
                IrStatementKind::For {
                    binding: symbol,
                    iterable,
                    body,
                }
            }
            StatementKind::Match(statement) => self.check_match(statement),
        };
        IrStatement {
            kind,
            span: statement.span,
        }
    }

    fn check_match(&mut self, statement: &MatchStatement) -> IrStatementKind {
        let subject = self.check_expression(&statement.subject, None);
        let mut covered = BTreeSet::new();
        let mut wildcard = false;
        let mut arms = Vec::new();
        for arm in &statement.arms {
            self.scopes.push(BTreeMap::new());
            let pattern =
                self.check_pattern(&arm.pattern, &subject.r#type, &mut covered, &mut wildcard);
            let body = self.check_statements(&arm.body);
            self.scopes.pop();
            arms.push(IrMatchArm {
                pattern,
                body,
                span: arm.span,
            });
        }
        if let Type::Enum(owner) = subject.r#type
            && !wildcard
        {
            let variants = self.enum_variants.get(&owner).cloned().unwrap_or_default();
            let missing: Vec<_> = variants
                .iter()
                .filter(|variant| !covered.contains(&variant.symbol))
                .map(|variant| variant.name.clone())
                .collect();
            if !missing.is_empty() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "M013112",
                        statement.subject.span,
                        "match does not cover every enum variant",
                    )
                    .with_primary_label(format!("missing: {}", missing.join(", ")))
                    .with_note("add the missing `case` arms or an explicit `else` arm"),
                );
            }
        }
        IrStatementKind::Match { subject, arms }
    }

    fn check_pattern(
        &mut self,
        pattern: &Pattern,
        subject_type: &Type,
        covered: &mut BTreeSet<SymbolId>,
        wildcard: &mut bool,
    ) -> IrPattern {
        match &pattern.kind {
            PatternKind::Wildcard => {
                *wildcard = true;
                IrPattern::Wildcard
            }
            PatternKind::Binding(name) => {
                *wildcard = true;
                IrPattern::Binding(self.declare_local(
                    name,
                    SymbolKind::Local,
                    subject_type.clone(),
                    false,
                ))
            }
            PatternKind::Literal(literal) => {
                let actual = self.literal_type(literal, None, pattern.span);
                if !Self::is_assignable(&actual, subject_type) {
                    self.type_mismatch(pattern.span, &actual, subject_type, "match pattern");
                }
                IrPattern::Literal(literal.clone())
            }
            PatternKind::Variant { path, bindings } => {
                let Some(variant) = self.resolve_variant(path, subject_type) else {
                    return IrPattern::Error;
                };
                covered.insert(variant.symbol);
                if bindings.len() != variant.fields.len() {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "M013103",
                            pattern.span,
                            format!(
                                "variant `{}` has {} field(s), but the pattern binds {}",
                                variant.name,
                                variant.fields.len(),
                                bindings.len()
                            ),
                        )
                        .with_primary_label("pattern field count does not match the variant"),
                    );
                }
                let binding_symbols = bindings
                    .iter()
                    .enumerate()
                    .map(|(index, binding)| {
                        self.declare_local(
                            binding,
                            SymbolKind::Local,
                            variant.fields.get(index).cloned().unwrap_or(Type::Error),
                            false,
                        )
                    })
                    .collect();
                IrPattern::Variant {
                    symbol: variant.symbol,
                    bindings: binding_symbols,
                }
            }
        }
    }

    fn resolve_variant(&mut self, path: &[Name], subject_type: &Type) -> Option<VariantInfo> {
        let owner = match subject_type {
            Type::Enum(owner) => *owner,
            other => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "M013102",
                        path.first().map_or(self.module.span, |part| part.span),
                        format!("{} does not have enum variants", self.describe_type(other)),
                    )
                    .with_primary_label("variant pattern requires an enum subject"),
                );
                return None;
            }
        };
        let requested_owner = if path.len() > 1 {
            self.type_names.get(&path[0].value).and_then(|r#type| {
                if let Type::Enum(id) = r#type {
                    Some(*id)
                } else {
                    None
                }
            })
        } else {
            Some(owner)
        };
        let name = path.last()?.value.as_str();
        let found = requested_owner.and_then(|requested_owner| {
            self.enum_variants
                .get(&requested_owner)
                .and_then(|variants| variants.iter().find(|variant| variant.name == name))
                .cloned()
        });
        let Some(variant) = found else {
            self.diagnostics.push(
                Diagnostic::error(
                    "M013002",
                    path.last().map_or(self.module.span, |part| part.span),
                    format!("unknown enum variant `{name}`"),
                )
                .with_primary_label("variant is not declared for this enum"),
            );
            return None;
        };
        if variant.owner != owner {
            self.diagnostics.push(
                Diagnostic::error(
                    "M013101",
                    path[0].span,
                    "enum variant belongs to a different enum",
                )
                .with_primary_label("variant cannot match this subject type"),
            );
            return None;
        }
        Some(variant)
    }

    fn check_module_assertion(&mut self, assertion: &Assertion) {
        self.check_expression(&assertion.condition, Some(&Type::Bool));
        match self.evaluate_constant(&assertion.condition) {
            Some(ConstantValue::Bool(true)) => {}
            Some(ConstantValue::Bool(false)) => self.diagnostics.push(
                Diagnostic::error("M013116", assertion.span, "compile-time assertion failed")
                    .with_primary_label("this condition evaluates to false"),
            ),
            _ => self.diagnostics.push(
                Diagnostic::error(
                    "M013118",
                    assertion.span,
                    "module assertion must be decidable at compile time",
                )
                .with_primary_label("condition is not a supported constant expression"),
            ),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn check_expression(
        &mut self,
        expression: &Expression,
        expected: Option<&Type>,
    ) -> IrExpression {
        let mut checked = match &expression.kind {
            ExpressionKind::Literal(literal) => IrExpression {
                kind: IrExpressionKind::Literal(literal.clone()),
                r#type: self.literal_type(literal, expected, expression.span),
                span: expression.span,
            },
            ExpressionKind::Name(path) => self.check_name(path, expression.span),
            ExpressionKind::Asset(name) => self.check_asset(name),
            ExpressionKind::Array(elements) => {
                self.check_array(elements, expected, expression.span)
            }
            ExpressionKind::Unary { operator, operand } => {
                let operand = self.check_expression(
                    operand,
                    (*operator == UnaryOperator::Not).then_some(&Type::Bool),
                );
                let r#type = match operator {
                    UnaryOperator::Not if operand.r#type == Type::Bool => Type::Bool,
                    UnaryOperator::BitNot if operand.r#type == Type::Int => Type::Int,
                    UnaryOperator::Negate | UnaryOperator::Positive
                        if operand.r#type.is_numeric() =>
                    {
                        operand.r#type.clone()
                    }
                    _ if operand.r#type.is_error() => Type::Error,
                    _ => {
                        self.invalid_operator(expression.span, "unary operator", &operand.r#type);
                        Type::Error
                    }
                };
                IrExpression {
                    kind: IrExpressionKind::Unary {
                        operator: *operator,
                        operand: Box::new(operand),
                    },
                    r#type,
                    span: expression.span,
                }
            }
            ExpressionKind::Binary {
                left,
                operator,
                right,
            } => self.check_binary(left, *operator, right, expression.span),
            ExpressionKind::Call { callee, arguments } => {
                self.check_call(callee, arguments, expression.span)
            }
            ExpressionKind::Field { subject, field } => {
                self.check_field(subject, field, expression.span)
            }
            ExpressionKind::Index { subject, index } => {
                let subject = self.check_expression(subject, None);
                let index = self.check_expression(index, Some(&Type::Int));
                let r#type = match &subject.r#type {
                    Type::Array { element, .. } | Type::List { element, .. } => (**element).clone(),
                    Type::Text => Type::Int,
                    Type::Error => Type::Error,
                    other => {
                        self.invalid_operator(expression.span, "indexing", other);
                        Type::Error
                    }
                };
                IrExpression {
                    kind: IrExpressionKind::Index {
                        subject: Box::new(subject),
                        index: Box::new(index),
                    },
                    r#type,
                    span: expression.span,
                }
            }
        };
        if let Some(expected) = expected
            && !Self::is_assignable(&checked.r#type, expected)
        {
            if matches!(
                (&checked.r#type, expected),
                (Type::Asset(_), Type::Asset(_))
            ) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "M013105",
                        expression.span,
                        format!(
                            "asset has type {}, expected {}",
                            self.describe_type(&checked.r#type),
                            self.describe_type(expected)
                        ),
                    )
                    .with_primary_label("wrong asset kind for this use"),
                );
            } else {
                self.type_mismatch(expression.span, &checked.r#type, expected, "expression");
            }
            checked.r#type = Type::Error;
        }
        checked
    }

    fn check_name(&mut self, path: &[Name], span: Span) -> IrExpression {
        if path.len() != 1 {
            self.diagnostics.push(
                Diagnostic::error("M013002", span, "qualified value name is not resolved here")
                    .with_primary_label("use a module member or local name"),
            );
            return error_expression(span);
        }
        let name = &path[0];
        let Some(symbol) = self.lookup(&name.value) else {
            self.diagnostics.push(
                Diagnostic::error(
                    "M013002",
                    name.span,
                    format!("unknown name `{}`", name.value),
                )
                .with_primary_label("no declaration with this name is visible here"),
            );
            return error_expression(span);
        };
        IrExpression {
            kind: IrExpressionKind::Load(symbol),
            r#type: self.symbol(symbol).r#type.clone(),
            span,
        }
    }

    fn install_memory_builtins(&mut self) {
        self.builtin(
            "font_print",
            vec![
                Type::Asset(AssetKind::Font),
                Type::Text,
                Type::Int,
                Type::Int,
                Type::Color,
            ],
            Type::Unit,
        );
        self.builtin("visual_id", vec![Type::Text], Type::Int);
        self.builtin("audio_id", vec![Type::Text], Type::Int);
        for (name, count, return_type) in [
            ("mem_read", 1, Type::Int),
            ("mem_read16", 1, Type::Int),
            ("mem_write", 2, Type::Unit),
            ("mem_write16", 2, Type::Unit),
            ("mem_copy", 3, Type::Unit),
            ("mem_fill", 3, Type::Unit),
            ("save_commit", 0, Type::Unit),
        ] {
            self.builtin(name, vec![Type::Int; count], return_type);
        }
    }

    fn is_raster_builtin(name: &str) -> bool {
        matches!(
            name,
            "pal"
                | "raster_scroll"
                | "mem_read"
                | "mem_read16"
                | "mem_write"
                | "mem_write16"
                | "mem_copy"
                | "mem_fill"
                | "visual_id"
                | "audio_id"
        )
    }

    fn check_asset(&mut self, name: &Name) -> IrExpression {
        if let Some(definition) = self.assets.get(&name.value) {
            IrExpression {
                kind: IrExpressionKind::Asset {
                    name: name.value.clone(),
                    kind: definition.kind,
                },
                r#type: Type::Asset(definition.kind),
                span: name.span,
            }
        } else {
            self.diagnostics.push(
                Diagnostic::error(
                    "M013104",
                    name.span,
                    format!("asset `#{}` is not declared", name.value),
                )
                .with_primary_label("missing named asset"),
            );
            error_expression(name.span)
        }
    }

    fn check_array(
        &mut self,
        elements: &[Expression],
        expected: Option<&Type>,
        span: Span,
    ) -> IrExpression {
        let contextual_element = match expected {
            Some(Type::Array { element, .. } | Type::List { element, .. }) => Some(&**element),
            _ => None,
        };
        let checked: Vec<_> = elements
            .iter()
            .map(|element| self.check_expression(element, contextual_element))
            .collect();
        let inferred = checked
            .first()
            .map_or(Type::Unknown, |element| element.r#type.clone());
        for element in &checked {
            if !Self::is_assignable(&element.r#type, &inferred)
                && !Self::is_assignable(&inferred, &element.r#type)
            {
                self.type_mismatch(element.span, &element.r#type, &inferred, "array element");
            }
        }
        let length = u32::try_from(elements.len()).unwrap_or(u32::MAX);
        let r#type = match expected {
            Some(Type::List { element, capacity }) if length <= *capacity => Type::List {
                element: element.clone(),
                capacity: *capacity,
            },
            Some(Type::Array {
                element,
                length: expected_length,
            }) if length == *expected_length => Type::Array {
                element: element.clone(),
                length,
            },
            _ => Type::Array {
                element: Box::new(inferred),
                length,
            },
        };
        IrExpression {
            kind: IrExpressionKind::Array(checked),
            r#type,
            span,
        }
    }

    fn check_binary(
        &mut self,
        left: &Expression,
        operator: BinaryOperator,
        right: &Expression,
        span: Span,
    ) -> IrExpression {
        let left = self.check_expression(left, None);
        let right = self.check_expression(right, None);
        let r#type = match operator {
            BinaryOperator::Or | BinaryOperator::And
                if left.r#type == Type::Bool && right.r#type == Type::Bool =>
            {
                Type::Bool
            }
            BinaryOperator::Equal | BinaryOperator::NotEqual
                if Self::is_assignable(&left.r#type, &right.r#type)
                    || Self::is_assignable(&right.r#type, &left.r#type) =>
            {
                Type::Bool
            }
            BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual
                if left.r#type.is_numeric() && right.r#type.is_numeric() =>
            {
                Type::Bool
            }
            BinaryOperator::Range if left.r#type == Type::Int && right.r#type == Type::Int => {
                Type::Range
            }
            op if op.bitwise_call().is_some()
                && left.r#type == Type::Int
                && right.r#type == Type::Int =>
            {
                Type::Int
            }
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
                if left.r#type.is_numeric() && right.r#type.is_numeric() =>
            {
                if left.r#type == Type::Num || right.r#type == Type::Num {
                    Type::Num
                } else {
                    Type::Int
                }
            }
            BinaryOperator::Add if left.r#type == Type::Text && right.r#type == Type::Text => {
                Type::Text
            }
            BinaryOperator::Remainder if left.r#type == Type::Int && right.r#type == Type::Int => {
                Type::Int
            }
            _ if left.r#type.is_error() || right.r#type.is_error() => Type::Error,
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "M013102",
                        span,
                        format!(
                            "operator {operator:?} is not defined for {} and {}",
                            self.describe_type(&left.r#type),
                            self.describe_type(&right.r#type)
                        ),
                    )
                    .with_primary_label("incompatible operand types"),
                );
                Type::Error
            }
        };
        IrExpression {
            kind: IrExpressionKind::Binary {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            },
            r#type,
            span,
        }
    }

    fn check_call(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
        span: Span,
    ) -> IrExpression {
        let callee = self.check_expression(callee, None);
        let Type::Function(signature) = callee.r#type.clone() else {
            if !callee.r#type.is_error() {
                self.diagnostics.push(
                    Diagnostic::error(
                        "M013103",
                        callee.span,
                        format!("{} is not callable", self.describe_type(&callee.r#type)),
                    )
                    .with_primary_label("expected a function, task, or constructor"),
                );
            }
            return error_expression(span);
        };
        let argument_count = u32::try_from(arguments.len()).unwrap_or(u32::MAX);
        let maximum = u32::try_from(signature.parameters.len()).unwrap_or(u32::MAX);
        if argument_count < signature.required_parameters || argument_count > maximum {
            let expectation = if signature.required_parameters == maximum {
                maximum.to_string()
            } else {
                format!("{} to {maximum}", signature.required_parameters)
            };
            self.diagnostics.push(
                Diagnostic::error(
                    "M013103",
                    span,
                    format!(
                        "call expects {expectation} argument(s), but received {}",
                        arguments.len()
                    ),
                )
                .with_primary_label("argument count does not match the callable signature"),
            );
        }
        let IrExpressionKind::Load(callee_symbol) = callee.kind else {
            self.diagnostics.push(
                Diagnostic::error(
                    "M013103",
                    callee.span,
                    "call target must be a non-capturing function reference",
                )
                .with_primary_label("dynamic host or closure calls are not available"),
            );
            return error_expression(span);
        };
        let callee_name = self.symbol(callee_symbol).name.clone();
        let special_builtin = self.symbol(callee_symbol).kind == SymbolKind::Builtin
            && matches!(
                callee_name.as_str(),
                "some" | "is_some" | "unwrap_or" | "abs" | "min" | "max" | "clamp" | "list_length"
            );
        let arguments = arguments
            .iter()
            .enumerate()
            .map(|(index, argument)| {
                let expected = if special_builtin {
                    None
                } else {
                    signature.parameters.get(index)
                };
                self.check_expression(argument, expected)
            })
            .collect::<Vec<_>>();
        let return_type = if special_builtin {
            self.check_special_builtin(&callee_name, &arguments, span)
        } else {
            (*signature.return_type).clone()
        };
        if self.current_routine == Some(RoutineKind::Callback(CallbackKind::Raster)) {
            let symbol = self.symbol(callee_symbol);
            if symbol.kind != SymbolKind::Builtin || !Self::is_raster_builtin(&symbol.name) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "M013121",
                        callee.span,
                        "raster callbacks may call only palette, raster scroll and hardware memory operations",
                    )
                    .with_primary_label("this call is outside the constrained raster facility"),
                );
            }
        }
        if signature.task && self.allowed_task_call_start != Some(span.start) {
            self.diagnostics.push(
                Diagnostic::error("M013111", span, "tasks must be launched with `start`")
                    .with_primary_label("direct task calls are not expressions"),
            );
        }
        IrExpression {
            kind: IrExpressionKind::Call {
                callee: callee_symbol,
                arguments,
            },
            r#type: return_type,
            span,
        }
    }

    fn check_special_builtin(
        &mut self,
        name: &str,
        arguments: &[IrExpression],
        call_span: Span,
    ) -> Type {
        match name {
            "list_length" => {
                if let Some(argument) = arguments.first()
                    && !matches!(
                        argument.r#type,
                        Type::List { .. } | Type::Array { .. } | Type::Error
                    )
                {
                    self.diagnostics.push(Diagnostic::error(
                        "M013101",
                        argument.span,
                        "list_length requires a fixed-capacity list or array",
                    ));
                }
                Type::Int
            }
            "abs" | "min" | "max" | "clamp" => {
                let mut result = Type::Int;
                for argument in arguments {
                    if !argument.r#type.is_numeric() && !argument.r#type.is_error() {
                        self.type_mismatch(
                            argument.span,
                            &argument.r#type,
                            &Type::Num,
                            "numeric helper argument",
                        );
                        result = Type::Error;
                    } else if argument.r#type == Type::Num && result != Type::Error {
                        result = Type::Num;
                    }
                }
                result
            }
            "some" => Type::Option(Box::new(
                arguments
                    .first()
                    .map_or(Type::Unknown, |argument| argument.r#type.clone()),
            )),
            "is_some" => {
                if let Some(argument) = arguments.first()
                    && !matches!(argument.r#type, Type::Option(_) | Type::Error)
                {
                    self.type_mismatch(
                        argument.span,
                        &argument.r#type,
                        &Type::Option(Box::new(Type::Unknown)),
                        "option argument",
                    );
                }
                Type::Bool
            }
            "unwrap_or" => {
                let element = match arguments.first().map(|argument| &argument.r#type) {
                    Some(Type::Option(element)) => (**element).clone(),
                    Some(Type::Error) | None => Type::Unknown,
                    Some(actual) => {
                        self.type_mismatch(
                            arguments
                                .first()
                                .map_or(call_span, |argument| argument.span),
                            actual,
                            &Type::Option(Box::new(Type::Unknown)),
                            "option argument",
                        );
                        Type::Unknown
                    }
                };
                if let Some(fallback) = arguments.get(1)
                    && !Self::is_assignable(&fallback.r#type, &element)
                {
                    self.type_mismatch(
                        fallback.span,
                        &fallback.r#type,
                        &element,
                        "fallback expression",
                    );
                }
                element
            }
            _ => Type::Unknown,
        }
    }

    fn check_field(&mut self, subject: &Expression, field: &Name, span: Span) -> IrExpression {
        if let ExpressionKind::Name(path) = &subject.kind
            && path.len() == 1
            && let Some(symbol) = self.lookup(&path[0].value)
            && self.symbol(symbol).kind == SymbolKind::Enum
            && let Some(variant) = self
                .enum_variants
                .get(&symbol)
                .and_then(|variants| variants.iter().find(|variant| variant.name == field.value))
        {
            return IrExpression {
                kind: IrExpressionKind::Load(variant.symbol),
                r#type: self.symbol(variant.symbol).r#type.clone(),
                span,
            };
        }

        let subject = self.check_expression(subject, None);
        let field_type = match &subject.r#type {
            Type::Record(symbol) => self
                .record_fields
                .get(symbol)
                .and_then(|fields| {
                    fields
                        .iter()
                        .find(|candidate| candidate.name == field.value)
                })
                .map(|field| field.r#type.clone()),
            Type::Vec2 if matches!(field.value.as_str(), "x" | "y") => Some(Type::Num),
            Type::Rect if matches!(field.value.as_str(), "x" | "y" | "w" | "h") => Some(Type::Num),
            Type::Module => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "M013007",
                        field.span,
                        "imported members require project-level module analysis",
                    )
                    .with_primary_label("module member cannot be resolved in an isolated module"),
                );
                Some(Type::Error)
            }
            _ => None,
        };
        let Some(r#type) = field_type else {
            self.diagnostics.push(
                Diagnostic::error(
                    "M013002",
                    field.span,
                    format!(
                        "type {} has no field `{}`",
                        self.describe_type(&subject.r#type),
                        field.value
                    ),
                )
                .with_primary_label("unknown field"),
            );
            return error_expression(span);
        };
        IrExpression {
            kind: IrExpressionKind::Field {
                subject: Box::new(subject),
                field: field.value.clone(),
            },
            r#type,
            span,
        }
    }

    fn lower_place(&mut self, expression: &Expression) -> (IrPlace, Type) {
        if !self.root_is_mutable(expression) {
            self.diagnostics.push(
                Diagnostic::error("M013106", expression.span, "assignment target is immutable")
                    .with_primary_label("declare a local with `var` or assign to `state`"),
            );
        }
        match &expression.kind {
            ExpressionKind::Name(path) if path.len() == 1 => {
                let checked = self.check_name(path, expression.span);
                if let IrExpressionKind::Load(symbol) = checked.kind {
                    (IrPlace::Symbol(symbol), checked.r#type)
                } else {
                    (IrPlace::Error, Type::Error)
                }
            }
            ExpressionKind::Field { subject, field } => {
                let checked = self.check_field(subject, field, expression.span);
                let r#type = checked.r#type.clone();
                if let IrExpressionKind::Field { subject, field } = checked.kind {
                    (IrPlace::Field { subject, field }, r#type)
                } else {
                    (IrPlace::Error, Type::Error)
                }
            }
            ExpressionKind::Index { subject, index } => {
                let checked = self.check_expression(expression, None);
                let r#type = checked.r#type.clone();
                if let IrExpressionKind::Index { subject, index } = checked.kind {
                    if subject.r#type == Type::Text {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "M013107",
                                expression.span,
                                "Text indexes are immutable",
                            )
                            .with_primary_label("assign a new Text value instead"),
                        );
                        return (IrPlace::Error, Type::Error);
                    }
                    (IrPlace::Index { subject, index }, r#type)
                } else {
                    let _ = (subject, index);
                    (IrPlace::Error, Type::Error)
                }
            }
            _ => {
                self.diagnostics.push(
                    Diagnostic::error(
                        "M013107",
                        expression.span,
                        "expression cannot be assigned to",
                    )
                    .with_primary_label(
                        "expected a state/local, record field, or collection index",
                    ),
                );
                (IrPlace::Error, Type::Error)
            }
        }
    }

    fn root_is_mutable(&self, expression: &Expression) -> bool {
        match &expression.kind {
            ExpressionKind::Name(path) if path.len() == 1 => self
                .lookup(&path[0].value)
                .is_some_and(|symbol| self.symbol(symbol).mutable),
            ExpressionKind::Field { subject, .. } | ExpressionKind::Index { subject, .. } => {
                self.root_is_mutable(subject)
            }
            _ => false,
        }
    }

    fn literal_type(&mut self, literal: &Literal, expected: Option<&Type>, span: Span) -> Type {
        match literal {
            Literal::Int(value) if expected == Some(&Type::Color) => {
                if (0..256).contains(value) {
                    Type::Color
                } else {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "M013119",
                            span,
                            "colour index must be between 0 and 255",
                        )
                        .with_primary_label("outside the fixed master palette"),
                    );
                    Type::Error
                }
            }
            Literal::Int(_) if expected == Some(&Type::Num) => Type::Num,
            Literal::Int(_) => Type::Int,
            Literal::Num(_) => Type::Num,
            Literal::Bool(_) => Type::Bool,
            Literal::Text(_) => Type::Text,
            Literal::None => match expected {
                Some(Type::Option(element)) => Type::Option(element.clone()),
                _ => Type::Option(Box::new(Type::Unknown)),
            },
            Literal::Frames(_) | Literal::Seconds(_) => Type::Duration,
        }
    }

    fn evaluate_constant(&self, expression: &Expression) -> Option<ConstantValue> {
        match &expression.kind {
            ExpressionKind::Literal(Literal::Int(value)) => Some(ConstantValue::Int(*value)),
            ExpressionKind::Literal(Literal::Num(value)) => Some(ConstantValue::Num(*value)),
            ExpressionKind::Literal(Literal::Bool(value)) => Some(ConstantValue::Bool(*value)),
            ExpressionKind::Literal(Literal::Text(value)) => {
                Some(ConstantValue::Text(value.clone()))
            }
            ExpressionKind::Literal(Literal::None) => Some(ConstantValue::None),
            ExpressionKind::Name(path) if path.len() == 1 => self
                .lookup(&path[0].value)
                .and_then(|symbol| self.constant_values.get(&symbol))
                .cloned(),
            ExpressionKind::Unary { operator, operand } => {
                let operand = self.evaluate_constant(operand)?;
                match (operator, operand) {
                    (UnaryOperator::Not, ConstantValue::Bool(value)) => {
                        Some(ConstantValue::Bool(!value))
                    }
                    (UnaryOperator::Negate, ConstantValue::Int(value)) => {
                        value.checked_neg().map(ConstantValue::Int)
                    }
                    (UnaryOperator::BitNot, ConstantValue::Int(value)) => {
                        constant_bits(value).map(|value| {
                            ConstantValue::Int(i64::from(i32::from_le_bytes(
                                (!value).to_le_bytes(),
                            )))
                        })
                    }
                    (UnaryOperator::Negate, ConstantValue::Num(value)) => {
                        Some(ConstantValue::Num(-value))
                    }
                    (
                        UnaryOperator::Positive,
                        value @ (ConstantValue::Int(_) | ConstantValue::Num(_)),
                    ) => Some(value),
                    _ => None,
                }
            }
            ExpressionKind::Binary {
                left,
                operator,
                right,
            } => Self::evaluate_constant_binary(
                self.evaluate_constant(left)?,
                *operator,
                self.evaluate_constant(right)?,
            ),
            _ => None,
        }
    }

    fn evaluate_constant_binary(
        left: ConstantValue,
        operator: BinaryOperator,
        right: ConstantValue,
    ) -> Option<ConstantValue> {
        match (left, operator, right) {
            (ConstantValue::Int(left), op, ConstantValue::Int(right))
                if op.bitwise_call().is_some() =>
            {
                let left = constant_bits(left)?;
                let right_bits = constant_bits(right)?;
                let shift = u32::try_from(right).ok().filter(|value| *value < 32);
                let value = match op {
                    BinaryOperator::BitAnd => left & right_bits,
                    BinaryOperator::BitOr => left | right_bits,
                    BinaryOperator::BitXor => left ^ right_bits,
                    BinaryOperator::ShiftLeft => left.wrapping_shl(shift?),
                    BinaryOperator::ShiftRight => u32::from_le_bytes(
                        (i32::from_le_bytes(left.to_le_bytes()) >> shift?).to_le_bytes(),
                    ),
                    BinaryOperator::ShiftRightUnsigned => {
                        return Some(ConstantValue::Int(i64::from(left >> shift?)));
                    }
                    _ => return None,
                };
                Some(ConstantValue::Int(i64::from(i32::from_le_bytes(
                    value.to_le_bytes(),
                ))))
            }
            (ConstantValue::Bool(left), BinaryOperator::And, ConstantValue::Bool(right)) => {
                Some(ConstantValue::Bool(left && right))
            }
            (ConstantValue::Bool(left), BinaryOperator::Or, ConstantValue::Bool(right)) => {
                Some(ConstantValue::Bool(left || right))
            }
            (ConstantValue::Int(left), BinaryOperator::Add, ConstantValue::Int(right)) => {
                left.checked_add(right).map(ConstantValue::Int)
            }
            (ConstantValue::Int(left), BinaryOperator::Subtract, ConstantValue::Int(right)) => {
                left.checked_sub(right).map(ConstantValue::Int)
            }
            (ConstantValue::Int(left), BinaryOperator::Multiply, ConstantValue::Int(right)) => {
                left.checked_mul(right).map(ConstantValue::Int)
            }
            (ConstantValue::Int(left), BinaryOperator::Divide, ConstantValue::Int(right)) => {
                left.checked_div(right).map(ConstantValue::Int)
            }
            (ConstantValue::Int(left), BinaryOperator::Remainder, ConstantValue::Int(right)) => {
                left.checked_rem(right).map(ConstantValue::Int)
            }
            (ConstantValue::Num(left), BinaryOperator::Add, ConstantValue::Num(right)) => {
                Some(ConstantValue::Num(left + right))
            }
            (ConstantValue::Num(left), BinaryOperator::Subtract, ConstantValue::Num(right)) => {
                Some(ConstantValue::Num(left - right))
            }
            (ConstantValue::Num(left), BinaryOperator::Multiply, ConstantValue::Num(right)) => {
                Some(ConstantValue::Num(left * right))
            }
            (ConstantValue::Num(left), BinaryOperator::Divide, ConstantValue::Num(right))
                if right != 0.0 =>
            {
                Some(ConstantValue::Num(left / right))
            }
            (ConstantValue::Text(left), BinaryOperator::Add, ConstantValue::Text(right)) => {
                Some(ConstantValue::Text(left + &right))
            }
            (left, BinaryOperator::Equal, right) => {
                Some(ConstantValue::Bool(constant_equal(&left, &right)))
            }
            (left, BinaryOperator::NotEqual, right) => {
                Some(ConstantValue::Bool(!constant_equal(&left, &right)))
            }
            (ConstantValue::Int(left), operator, ConstantValue::Int(right)) => {
                compare_ord(&left, operator, &right).map(ConstantValue::Bool)
            }
            (ConstantValue::Num(left), operator, ConstantValue::Num(right)) => {
                compare_partial(&left, operator, &right).map(ConstantValue::Bool)
            }
            _ => None,
        }
    }

    fn resolve_type(&mut self, node: &TypeNode) -> Type {
        match &node.kind {
            TypeKind::Named(path) => {
                if path.len() != 1 {
                    self.unknown_type(node.span, "qualified types require project analysis");
                    return Type::Error;
                }
                self.named_type(&path[0].value).unwrap_or_else(|| {
                    self.unknown_type(node.span, &format!("unknown type `{}`", path[0].value));
                    Type::Error
                })
            }
            TypeKind::Generic { name, arguments } => {
                let Some(name) = name.first().map(|part| part.value.as_str()) else {
                    return Type::Error;
                };
                match (name, arguments.as_slice()) {
                    ("Option", [TypeArgument::Type(element)]) => {
                        Type::Option(Box::new(self.resolve_type(element)))
                    }
                    (
                        "List",
                        [
                            TypeArgument::Type(element),
                            TypeArgument::Capacity(capacity),
                        ],
                    ) => {
                        self.check_capacity(capacity.value, capacity.span);
                        Type::List {
                            element: Box::new(self.resolve_type(element)),
                            capacity: capacity.value,
                        }
                    }
                    _ => {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "M013004",
                                node.span,
                                format!("invalid type arguments for `{name}`"),
                            )
                            .with_primary_label(
                                "expected `Option[T]` or fixed-capacity `List[T, N]`",
                            ),
                        );
                        Type::Error
                    }
                }
            }
            TypeKind::Array { element, length } => {
                self.check_capacity(*length, node.span);
                Type::Array {
                    element: Box::new(self.resolve_type(element)),
                    length: *length,
                }
            }
        }
    }

    fn named_type(&self, name: &str) -> Option<Type> {
        Some(match name {
            "Unit" => Type::Unit,
            "Num" => Type::Num,
            "Int" => Type::Int,
            "Bool" => Type::Bool,
            "Text" => Type::Text,
            "Color" => Type::Color,
            "Vec2" => Type::Vec2,
            "Rect" => Type::Rect,
            "Controller" => Type::Controller,
            "Button" => Type::Button,
            "Duration" => Type::Duration,
            "Sprite" => Type::Asset(AssetKind::Sprite),
            "Animation" => Type::Asset(AssetKind::Animation),
            "TileSet" => Type::Asset(AssetKind::TileSet),
            "Map" => Type::Asset(AssetKind::Map),
            "Font" => Type::Asset(AssetKind::Font),
            "Sound" => Type::Asset(AssetKind::Sound),
            "Music" => Type::Asset(AssetKind::Music),
            _ => return self.type_names.get(name).cloned(),
        })
    }

    fn check_capacity(&mut self, capacity: u32, span: Span) {
        if capacity == 0 || capacity > 65_535 {
            self.diagnostics.push(
                Diagnostic::error(
                    "M013114",
                    span,
                    "fixed collection capacity must be between 1 and 65535",
                )
                .with_primary_label("invalid fixed capacity"),
            );
        }
    }

    fn unknown_type(&mut self, span: Span, message: &str) {
        self.diagnostics.push(
            Diagnostic::error("M013003", span, message)
                .with_primary_label("type is not declared in this module"),
        );
    }

    fn declare_global(
        &mut self,
        name: &Name,
        kind: SymbolKind,
        r#type: Type,
        mutable: bool,
    ) -> Option<SymbolId> {
        if let Some(existing) = self.globals.get(&name.value).copied() {
            let mut diagnostic = Diagnostic::error(
                "M013001",
                name.span,
                format!("name `{}` is already declared", name.value),
            )
            .with_primary_label("duplicate declaration");
            if let Some(first) = self.symbol(existing).defined_at {
                diagnostic = diagnostic.with_secondary(first, "first declared here");
            } else {
                diagnostic = diagnostic.with_note("this name is reserved by the MODL standard API");
            }
            self.diagnostics.push(diagnostic);
            return None;
        }
        let id = self.add_symbol(name.value.clone(), kind, r#type, mutable, Some(name.span));
        self.globals.insert(name.value.clone(), id);
        Some(id)
    }

    fn declare_local(
        &mut self,
        name: &Name,
        kind: SymbolKind,
        r#type: Type,
        mutable: bool,
    ) -> SymbolId {
        let existing = self
            .scopes
            .last()
            .and_then(|scope| scope.get(&name.value))
            .copied();
        if let Some(existing) = existing {
            let first = self.symbol(existing).defined_at.unwrap_or(name.span);
            self.duplicate(
                name.span,
                first,
                format!("name `{}` is declared twice in this scope", name.value),
            );
        }
        let symbol = self.add_symbol(name.value.clone(), kind, r#type, mutable, Some(name.span));
        if existing.is_none()
            && let Some(scope) = self.scopes.last_mut()
        {
            scope.insert(name.value.clone(), symbol);
        }
        symbol
    }

    fn add_symbol(
        &mut self,
        name: impl Into<String>,
        kind: SymbolKind,
        r#type: Type,
        mutable: bool,
        defined_at: Option<Span>,
    ) -> SymbolId {
        let id = SymbolId(u32::try_from(self.symbols.len()).unwrap_or(u32::MAX));
        self.symbols.push(Symbol {
            id,
            name: name.into(),
            kind,
            r#type,
            mutable,
            defined_at,
        });
        id
    }

    fn lookup(&self, name: &str) -> Option<SymbolId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
            .or_else(|| self.globals.get(name).copied())
    }

    fn symbol(&self, id: SymbolId) -> &Symbol {
        &self.symbols[usize::try_from(id.0).expect("symbol ID fits usize")]
    }

    fn symbol_mut(&mut self, id: SymbolId) -> &mut Symbol {
        &mut self.symbols[usize::try_from(id.0).expect("symbol ID fits usize")]
    }

    fn duplicate(&mut self, duplicate: Span, first: Span, message: String) {
        self.diagnostics.push(
            Diagnostic::error("M013001", duplicate, message)
                .with_primary_label("duplicate declaration")
                .with_secondary(first, "first declared here"),
        );
    }

    fn is_assignable(actual: &Type, expected: &Type) -> bool {
        if actual == expected || actual.is_error() || expected.is_error() {
            return true;
        }
        match (actual, expected) {
            (Type::Unknown, _) | (_, Type::Unknown) | (Type::Int, Type::Num) => true,
            (Type::Option(actual), Type::Option(expected)) => Self::is_assignable(actual, expected),
            (
                Type::Array {
                    element: actual,
                    length,
                },
                Type::List {
                    element: expected,
                    capacity,
                },
            ) => length <= capacity && Self::is_assignable(actual, expected),
            _ => false,
        }
    }

    fn type_mismatch(&mut self, span: Span, actual: &Type, expected: &Type, context: &str) {
        self.diagnostics.push(
            Diagnostic::error(
                "M013101",
                span,
                format!(
                    "{context} has type {}, expected {}",
                    self.describe_type(actual),
                    self.describe_type(expected)
                ),
            )
            .with_primary_label("type mismatch"),
        );
    }

    fn invalid_operator(&mut self, span: Span, operation: &str, r#type: &Type) {
        self.diagnostics.push(
            Diagnostic::error(
                "M013102",
                span,
                format!(
                    "{operation} is not defined for {}",
                    self.describe_type(r#type)
                ),
            )
            .with_primary_label("invalid operand type"),
        );
    }

    fn describe_type(&self, r#type: &Type) -> String {
        match r#type {
            Type::Unit => "Unit".to_owned(),
            Type::Num => "Num".to_owned(),
            Type::Int => "Int".to_owned(),
            Type::Bool => "Bool".to_owned(),
            Type::Text => "Text".to_owned(),
            Type::Color => "Color".to_owned(),
            Type::Vec2 => "Vec2".to_owned(),
            Type::Rect => "Rect".to_owned(),
            Type::Controller => "Controller".to_owned(),
            Type::Button => "Button".to_owned(),
            Type::Duration => "Duration".to_owned(),
            Type::Asset(kind) => kind.type_name().to_owned(),
            Type::Option(element) => format!("Option[{}]", self.describe_type(element)),
            Type::Array { element, length } => {
                format!("[{}, {length}]", self.describe_type(element))
            }
            Type::List { element, capacity } => {
                format!("List[{}, {capacity}]", self.describe_type(element))
            }
            Type::Record(symbol) | Type::Enum(symbol) => self.symbol(*symbol).name.clone(),
            Type::Function(signature) => format!(
                "fn({}) -> {}",
                signature
                    .parameters
                    .iter()
                    .map(|parameter| self.describe_type(parameter))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.describe_type(&signature.return_type)
            ),
            Type::Range => "Range".to_owned(),
            Type::Module => "module".to_owned(),
            Type::Unknown => "unknown".to_owned(),
            Type::Error => "error".to_owned(),
        }
    }
}

fn constant_bits(value: i64) -> Option<u32> {
    u32::try_from(value).ok().or_else(|| {
        i32::try_from(value)
            .ok()
            .map(|value| u32::from_le_bytes(value.to_le_bytes()))
    })
}

fn error_expression(span: Span) -> IrExpression {
    IrExpression {
        kind: IrExpressionKind::Error,
        r#type: Type::Error,
        span,
    }
}

#[allow(clippy::float_cmp)] // exact IEEE equality is the specified MODL operator behavior
fn constant_equal(left: &ConstantValue, right: &ConstantValue) -> bool {
    match (left, right) {
        (ConstantValue::Int(left), ConstantValue::Int(right)) => left == right,
        (ConstantValue::Num(left), ConstantValue::Num(right)) => left == right,
        (ConstantValue::Bool(left), ConstantValue::Bool(right)) => left == right,
        (ConstantValue::Text(left), ConstantValue::Text(right)) => left == right,
        (ConstantValue::None, ConstantValue::None) => true,
        _ => false,
    }
}

fn compare_ord<T: Ord>(left: &T, operator: BinaryOperator, right: &T) -> Option<bool> {
    Some(match operator {
        BinaryOperator::Less => left < right,
        BinaryOperator::LessEqual => left <= right,
        BinaryOperator::Greater => left > right,
        BinaryOperator::GreaterEqual => left >= right,
        _ => return None,
    })
}

fn compare_partial<T: PartialOrd>(left: &T, operator: BinaryOperator, right: &T) -> Option<bool> {
    Some(match operator {
        BinaryOperator::Less => left < right,
        BinaryOperator::LessEqual => left <= right,
        BinaryOperator::Greater => left > right,
        BinaryOperator::GreaterEqual => left >= right,
        _ => return None,
    })
}

fn block_guarantees_return(block: &Block) -> bool {
    block.last().is_some_and(|statement| match &statement.kind {
        StatementKind::Return(_) => true,
        StatementKind::If(statement) => {
            !statement.branches.is_empty()
                && statement
                    .branches
                    .iter()
                    .all(|branch| block_guarantees_return(&branch.body))
                && statement
                    .else_body
                    .as_ref()
                    .is_some_and(block_guarantees_return)
        }
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use crate::{AssetCatalog, AssetKind, FileId, SourceFile, analyze_module};

    fn analyze(source: &str, assets: &AssetCatalog) -> super::AnalysisOutput {
        analyze_module(&SourceFile::new(FileId(0), "test.modl", source), assets)
    }

    #[test]
    fn resolves_and_types_a_small_game() {
        let mut assets = AssetCatalog::default();
        assets.insert("hero", AssetKind::Sprite);
        let output = analyze(
            r"state score: Int = 0
fn twice(value: Int) -> Int:
  return value * 2
task flash():
  wait 2f
on update:
  score = twice(score)
  start flash()
on draw:
  sprite(#hero, 10, 10)
",
            &assets,
        );
        assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
        let ir = output.ir.expect("valid program has IR");
        assert_eq!(ir.globals.len(), 1);
        assert_eq!(ir.routines.len(), 4);
    }

    #[test]
    fn types_custom_font_drawing_against_the_asset_catalog() {
        let mut assets = AssetCatalog::default();
        assets.insert("tiny", AssetKind::Font);
        let output = analyze("on draw:\n  font_print(#tiny, \"AZ\", 1, 2, 9)\n", &assets);
        assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
        assert!(output.ir.is_some());
    }

    #[test]
    fn reports_missing_and_wrong_kind_assets() {
        let mut assets = AssetCatalog::default();
        assets.insert("level", AssetKind::Map);
        let output = analyze(
            "on draw:\n  sprite(#level, 0, 0)\n  sprite(#missing, 0, 0)\n",
            &assets,
        );
        let codes: Vec<_> = output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        assert!(codes.contains(&"M013104"));
        assert!(codes.contains(&"M013105"));
    }

    #[test]
    fn enforces_match_exhaustiveness_and_mutability() {
        let output = analyze(
            "enum Mode:\n  Title\n  Play\nstate mode: Mode = Mode.Title\non update:\n  let copy = mode\n  copy = Mode.Play\n  match mode:\n    case Mode.Title:\n      mode = Mode.Play\n",
            &AssetCatalog::default(),
        );
        assert!(
            output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "M013106")
        );
        assert!(
            output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "M013112")
        );
    }

    #[test]
    fn task_calls_require_the_start_statement() {
        let output = analyze(
            "task blink():\n  wait 1f\non update:\n  blink()\n",
            &AssetCatalog::default(),
        );
        assert!(output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "M013111" && diagnostic.message.contains("launched with `start`")
        }));
    }

    #[test]
    fn option_helpers_preserve_the_element_type_and_reject_mismatches() {
        let valid = analyze(
            "state saved: Option[Int] = none\non start:\n  saved = some(7)\n  let present = is_some(saved)\n  let value = unwrap_or(saved, 0)\n",
            &AssetCatalog::default(),
        );
        assert!(valid.diagnostics.is_empty(), "{:#?}", valid.diagnostics);

        let invalid = analyze(
            "on start:\n  let wrong_option = is_some(7)\n  let wrong_fallback = unwrap_or(some(7), \"seven\")\n",
            &AssetCatalog::default(),
        );
        assert_eq!(
            invalid
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "M013101")
                .count(),
            2
        );
    }

    #[test]
    fn memory_builtins_are_typed_and_reserved_like_the_rest_of_the_api() {
        let invalid = analyze("on draw:\n  mem_write(true, 7)\n", &AssetCatalog::default());
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "M013101")
        );
        let wrong_name = analyze(
            "on start:\n  let id = visual_id(7)\n  let sound_id = audio_id(7)\n",
            &AssetCatalog::default(),
        );
        assert!(
            wrong_name
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "M013101")
        );
        assert_eq!(
            wrong_name
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "M013101")
                .count(),
            2
        );
        let shadowed = analyze(
            "fn mem_read(value: Bool) -> Bool:\n  return value\non start:\n  assert mem_read(true), \"user function\"\n",
            &AssetCatalog::default(),
        );
        assert!(
            shadowed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "M013001")
        );
    }

    #[test]
    fn standard_api_metadata_matches_the_complete_compiler_symbol_table() {
        let analysis = analyze("", &AssetCatalog::default());
        let definitions = super::builtin_definitions();
        assert_eq!(definitions.len(), analysis.symbols.len());
        assert!(
            definitions
                .windows(2)
                .all(|pair| pair[0].name < pair[1].name)
        );
        for symbol in &analysis.symbols {
            let definition = definitions
                .iter()
                .find(|definition| definition.name == symbol.name)
                .unwrap();
            assert_eq!(
                definition.callable,
                matches!(symbol.r#type, crate::Type::Function(_))
            );
            assert!(definition.signature.contains(&symbol.name));
        }
        for name in [
            "font_print",
            "mem_copy",
            "audio_id",
            "save_commit",
            "sqrt",
            "text_split",
            "bit_ushr",
        ] {
            assert!(definitions.iter().any(|definition| definition.name == name));
        }
        assert_eq!(
            definitions
                .iter()
                .find(|definition| definition.name == "parse_num")
                .unwrap()
                .signature,
            "fn parse_num(Text) -> Option[Num]"
        );
    }

    #[test]
    fn bit_operators_share_word_rules_in_constants_and_runtime_ir() {
        let valid = analyze(
            "assert 1 << 2 + 1 == 8\nassert ~0 == -1\nassert -1 >>> 0 == 4294967295\nassert 1 | 2 ^ 3 & 6 == 1\non start:\n  let word = 1 << 31\n  assert word >> 31 == -1\n",
            &AssetCatalog::default(),
        );
        assert!(valid.diagnostics.is_empty(), "{:#?}", valid.diagnostics);
        let invalid = analyze(
            "on start:\n  let bad = 1 & true\n  let worse = ~1.5\n  let count = 1 << 2.0\n",
            &AssetCatalog::default(),
        );
        assert_eq!(
            invalid
                .diagnostics
                .iter()
                .filter(|d| d.code == "M013102")
                .count(),
            3
        );
        let overflow = analyze("assert 1 << 32 == 1\n", &AssetCatalog::default());
        assert!(!overflow.diagnostics.is_empty());
    }

    #[test]
    fn text_helpers_return_typed_options_and_bounded_collections() {
        let valid = analyze(
            "state number: Option[Int] = parse_int(\"ff\", 16)\nstate real: Option[Num] = parse_num(\"1.25\")\nstate parts: List[Text, 256] = text_split(\"a,b\", \",\")\non start:\n  let label = format_int(unwrap_or(number, 0), 10, 4)\n  assert list_length(parts) == 2\n",
            &AssetCatalog::default(),
        );
        assert!(valid.diagnostics.is_empty(), "{:#?}", valid.diagnostics);
        let invalid = analyze(
            "on start:\n  let bad = text_slice(1, 0, 1)\n  let not_a_list = list_length(\"hi\")\n  let wrong_fallback = unwrap_or(parse_int(\"7\", 10), false)\n",
            &AssetCatalog::default(),
        );
        assert_eq!(
            invalid
                .diagnostics
                .iter()
                .filter(|d| d.code == "M013101")
                .count(),
            3
        );
    }

    #[test]
    fn numeric_helpers_preserve_integer_types_and_reject_non_numeric_arguments() {
        let valid = analyze(
            "fn small() -> Int:\n  return min(abs(-7), clamp(12, 1, 10))\nfn rounded() -> Int:\n  return round(sqrt(max(small(), 1.5)))\n",
            &AssetCatalog::default(),
        );
        assert!(valid.diagnostics.is_empty(), "{:#?}", valid.diagnostics);
        let invalid = analyze(
            "fn not_int() -> Int:\n  return min(1.5, 2)\non start:\n  let bad = abs(true)\n  let worse = clamp(1, \"two\", 3)\n  let missing = atan2(1)\n",
            &AssetCatalog::default(),
        );
        assert_eq!(
            invalid
                .diagnostics
                .iter()
                .filter(|d| d.code == "M013101")
                .count(),
            3
        );
        assert!(invalid.diagnostics.iter().any(|d| d.code == "M013103"));
    }

    #[test]
    fn raster_callbacks_reject_calls_outside_the_display_list() {
        let output = analyze(
            "on raster(line: Int):\n  pal(1, 2)\n  raster_scroll(line, 0)\n  pixel(0, line, 3)\n",
            &AssetCatalog::default(),
        );
        assert_eq!(
            output
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "M013121")
                .count(),
            1
        );
    }
}
