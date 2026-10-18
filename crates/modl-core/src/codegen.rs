use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    CARTRIDGE_FORMAT_REVISION, LANGUAGE_REVISION,
    ast::{AssignmentOperator, BinaryOperator, Literal, UnaryOperator},
    diagnostic::Diagnostic,
    ir::{
        IrBranch, IrExpression, IrExpressionKind, IrMatchArm, IrModule, IrPattern, IrPlace,
        IrRoutine, IrStatement, IrStatementKind, RoutineKind, Symbol, SymbolKind,
    },
    sema::{AnalysisOutput, analyze_module},
    span::{LineColumn, SourceFile, Span},
    types::{AssetCatalog, SymbolId, Type},
};

/// Compiler output mode. Both modes consume identical typed IR and work-unit instrumentation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompileMode {
    Release,
    Debug,
}

/// A direct source relationship retained alongside the standard source map.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceRelationship {
    pub generated_line: u32,
    pub generated_column: u32,
    pub source_span: Span,
}

/// Static work charges inserted by the compiler.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkModel {
    pub function_entry: u32,
    pub loop_back_edge: u32,
    pub task_transition: u32,
    pub allocation: u32,
    pub console_call: u32,
}

impl Default for WorkModel {
    fn default() -> Self {
        Self {
            function_entry: 8,
            loop_back_edge: 4,
            task_transition: 4,
            allocation: 4,
            console_call: 2,
        }
    }
}

/// Generated JavaScript and inspectable debug/build metadata.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeneratedProgram {
    pub mode: CompileMode,
    pub javascript: String,
    pub source_map_json: String,
    pub relationships: Vec<SourceRelationship>,
    pub generated_bytes: u32,
    pub probe_count: u32,
    pub work_model: WorkModel,
}

/// Full compiler pipeline result. Generation is absent whenever diagnostics contain errors.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CompilationOutput {
    pub analysis: AnalysisOutput,
    pub generated: Option<GeneratedProgram>,
}

/// Maps spans in a synthetic linked project source back to the original project modules.
#[derive(Clone, Debug)]
pub(crate) struct SourceOriginMap {
    sources: Vec<SourceFile>,
    ranges: Vec<SourceOriginRange>,
}

#[derive(Clone, Debug)]
pub(crate) struct SourceOriginRange {
    pub linked_start: u32,
    pub linked_end: u32,
    pub file: crate::span::FileId,
    /// Original byte offset for every byte boundary in the rendered linked module.
    pub offsets: Vec<u32>,
}

impl SourceOriginMap {
    #[must_use]
    pub(crate) const fn new(sources: Vec<SourceFile>, ranges: Vec<SourceOriginRange>) -> Self {
        Self { sources, ranges }
    }

    fn map_span(&self, span: Span) -> Span {
        let Some(range) = self
            .ranges
            .iter()
            .find(|range| span.start >= range.linked_start && span.start < range.linked_end)
        else {
            return span;
        };
        let start = usize::try_from(span.start - range.linked_start).unwrap_or(usize::MAX);
        let end = usize::try_from(span.end.min(range.linked_end) - range.linked_start)
            .unwrap_or(usize::MAX);
        match (range.offsets.get(start), range.offsets.get(end)) {
            (Some(start), Some(end)) => Span::new(range.file, *start, *end),
            _ => span,
        }
    }

    fn source(&self, file: crate::span::FileId) -> Option<&SourceFile> {
        self.sources.iter().find(|source| source.id() == file)
    }
}

/// Runs source through parsing, resolution, type checking, typed IR, instrumentation, JavaScript
/// generation, and source-map generation.
#[must_use]
pub fn compile(source: &SourceFile, assets: &AssetCatalog, mode: CompileMode) -> CompilationOutput {
    compile_with_origins(source, assets, mode, None)
}

pub(crate) fn compile_with_origins(
    source: &SourceFile,
    assets: &AssetCatalog,
    mode: CompileMode,
    origins: Option<&SourceOriginMap>,
) -> CompilationOutput {
    let analysis = analyze_module(source, assets);
    let generated = if analysis.diagnostics.is_empty() {
        analysis
            .ir
            .as_ref()
            .map(|ir| Generator::new(source, &analysis.symbols, ir, mode, origins).generate())
    } else {
        None
    };
    CompilationOutput {
        analysis,
        generated,
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StandardSourceMap {
    version: u8,
    file: String,
    sources: Vec<String>,
    sources_content: Vec<String>,
    names: Vec<String>,
    mappings: String,
}

struct Generator<'input> {
    source: &'input SourceFile,
    symbols: &'input [Symbol],
    ir: &'input IrModule,
    mode: CompileMode,
    writer: Writer,
    symbol_index: BTreeMap<SymbolId, usize>,
    probe_count: u32,
    temporary: u32,
    task_programs: Vec<TaskProgram>,
    debug_locals: Vec<SymbolId>,
    work: WorkModel,
    origins: Option<&'input SourceOriginMap>,
}

impl<'input> Generator<'input> {
    fn new(
        source: &'input SourceFile,
        symbols: &'input [Symbol],
        ir: &'input IrModule,
        mode: CompileMode,
        origins: Option<&'input SourceOriginMap>,
    ) -> Self {
        let symbol_index = symbols
            .iter()
            .enumerate()
            .map(|(index, symbol)| (symbol.id, index))
            .collect();
        Self {
            source,
            symbols,
            ir,
            mode,
            writer: Writer::default(),
            symbol_index,
            probe_count: 0,
            temporary: 0,
            task_programs: Vec::new(),
            debug_locals: Vec::new(),
            work: WorkModel::default(),
            origins,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn generate(mut self) -> GeneratedProgram {
        self.task_programs = self
            .ir
            .routines
            .iter()
            .filter(|routine| routine.kind == RoutineKind::Task)
            .map(TaskProgram::lower)
            .collect();

        self.writer.line(
            format!(
                "// Generated by MODL compiler {} for {}; source remains in the cartridge.",
                env!("CARGO_PKG_VERSION"),
                LANGUAGE_REVISION
            ),
            None,
        );
        self.writer.line(
            format!(
                "export const metadata=Object.freeze({{language:{},format:{},mode:{}}});",
                js_string(LANGUAGE_REVISION),
                CARTRIDGE_FORMAT_REVISION,
                js_string(match self.mode {
                    CompileMode::Release => "release",
                    CompileMode::Debug => "debug",
                })
            ),
            None,
        );
        self.writer
            .line("export default function createCartridge(api){", None);
        self.writer.indent += 1;
        self.writer.line("\"use strict\";", None);
        self.writer.line("const state=Object.create(null);", None);
        self.writer.line("const tasks=[];", None);
        self.writer.line("const callStack=[];", None);
        self.writer.line("let nextTaskId=1;", None);
        self.writer.line(
            "const work=(units,start,end)=>api.work(units,{start,end});",
            None,
        );
        self.writer.line(
            "const integer=(value,start,end)=>{if(!Number.isSafeInteger(value))return api.fault(\"M019002\",\"integer result is outside the safe range\",{start,end});return value;};",
            None,
        );
        self.writer.line(
            "const divideInt=(left,right,start,end)=>{if(right===0)return api.fault(\"M019003\",\"integer division by zero\",{start,end});return integer(Math.trunc(left/right),start,end);};",
            None,
        );
        self.writer.line(
            format!(
                "const allocate=(value,start,end)=>{{work({},start,end);return value;}};",
                self.work.allocation
            ),
            None,
        );
        self.writer.line(
            "const boundedIndex=(key,limit,start,end)=>{if(!Number.isSafeInteger(key)||key<0||key>=limit)return api.fault(\"M019007\",\"collection index is outside its fixed capacity\",{start,end});return key;};",
            None,
        );
        self.writer.line(
            "const collectionRead=(value,key,limit,start,end)=>{key=boundedIndex(key,limit,start,end);if(!(key in value))return api.fault(\"M019008\",\"list element has not been initialized\",{start,end});return value[key];};",
            None,
        );
        self.writer.line(
            format!(
                "const collectionWrite=(value,key,limit,next,start,end)=>{{key=boundedIndex(key,limit,start,end);work({},start,end);value[key]=next;return next;}};",
                self.work.allocation
            ),
            None,
        );
        self.writer.line(
            "const textIndex=(value,key,start,end)=>api.call(\"text_at\",[value,key],{start,end});",
            None,
        );
        self.writer.line(
            "const optionSome=(value,start,end)=>allocate({__some:true,value},start,end);const optionIsSome=value=>value!==null;const optionUnwrap=(value,fallback)=>value===null?fallback:value.value;",
            None,
        );
        if !self.ir.enums.is_empty() {
            // enum values survive cloning; payload traversal is bounded by the ordinary work budget.
            self.writer.line(
                "const enumEqual=(a,b,start,end)=>{if(a.__enum!==b.__enum||a.__variant!==b.__variant)return false;work(a.fields.length,start,end);const pending=a.fields.map((value,index)=>[value,b.fields[index]]);while(pending.length){const [left,right]=pending.pop();if(left===right)continue;if(left===null||right===null||typeof left!==\"object\"||typeof right!==\"object\"||Array.isArray(left)!==Array.isArray(right))return false;const keys=Object.keys(left);work(keys.length,start,end);if(keys.length!==Object.keys(right).length)return false;for(const key of keys){if(!Object.hasOwn(right,key))return false;pending.push([left[key],right[key]]);}}return true;};",
                None,
            );
        }
        self.writer.line(
            "const iterable=(value,start,end)=>{if(!value?.__range)return value;const length=Math.max(0,value.end-value.start);work(length,start,end);return [...Array(length).keys()].map(index=>value.start+index);};",
            None,
        );
        self.writer.line(
            format!(
                "const consoleCall=(name,args,start,end)=>{{work({},start,end);return api.call(name,args,{{start,end}});}};",
                self.work.console_call
            ),
            None,
        );
        self.writer.line(
            if self.mode == CompileMode::Debug {
                "const enter=(name,sourceSpan)=>{callStack.push({name,sourceSpan});api.enter?.(name,sourceSpan);};"
            } else {
                "const enter=(name,start,end)=>{callStack.push({name,start,end});api.enter?.(name,{start,end});};"
            },
            None,
        );
        self.writer
            .line("const leave=()=>{callStack.pop();api.leave?.();};", None);

        self.generate_nominal_values();
        self.generate_routines();
        self.generate_tasks();
        self.generate_globals();
        self.generate_scheduler();
        self.generate_instance();

        self.writer.indent -= 1;
        self.writer.line("}", None);

        let mappings = self.writer.standard_mappings(self.source, self.origins);
        let source_files = self.origins.map_or_else(
            || vec![self.source],
            |origins| origins.sources.iter().collect(),
        );
        let source_map = StandardSourceMap {
            version: 3,
            file: "cartridge.js".to_owned(),
            sources: source_files
                .iter()
                .map(|source| source.name().to_owned())
                .collect(),
            sources_content: source_files
                .iter()
                .map(|source| source.text().to_owned())
                .collect(),
            names: Vec::new(),
            mappings,
        };
        let source_map_json =
            serde_json::to_string(&source_map).expect("source-map fields always serialize to JSON");
        let generated_bytes = u32::try_from(self.writer.output.len()).unwrap_or(u32::MAX);
        let relationships = self
            .writer
            .relationships
            .iter()
            .map(|relationship| SourceRelationship {
                generated_line: relationship.generated_line,
                generated_column: relationship.generated_column,
                source_span: self.map_span(relationship.source_span),
            })
            .collect();
        GeneratedProgram {
            mode: self.mode,
            javascript: self.writer.output,
            source_map_json,
            relationships,
            generated_bytes,
            probe_count: self.probe_count,
            work_model: self.work,
        }
    }

    fn generate_nominal_values(&mut self) {
        for record in self.ir.records.clone() {
            let parameters: Vec<_> = record
                .fields
                .iter()
                .enumerate()
                .map(|(index, _)| format!("a{index}"))
                .collect();
            let fields = record
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| format!("{}:a{index}", safe_property(&field.name)))
                .collect::<Vec<_>>()
                .join(",");
            let defaults = record
                .fields
                .iter()
                .enumerate()
                .filter_map(|(index, field)| {
                    field.default.as_ref().map(|default| {
                        format!(
                            "if(arguments.length<={index})a{index}={};",
                            self.expression(default, ValueContext::Routine)
                        )
                    })
                })
                .collect::<String>();
            self.writer.line(
                format!(
                    "function{} s{}({}){{work({},0,0);{defaults}return {{{fields}}};}}",
                    if self.mode == CompileMode::Debug {
                        "*"
                    } else {
                        ""
                    },
                    record.symbol.0,
                    parameters.join(","),
                    self.work.allocation
                ),
                None,
            );
        }
        for enumeration in &self.ir.enums {
            for variant in &enumeration.variants {
                if variant.fields.is_empty() {
                    self.writer.line(
                        format!(
                            "const s{}=Object.freeze({{__enum:{},__variant:{},fields:[]}});",
                            variant.symbol.0, enumeration.symbol.0, variant.symbol.0
                        ),
                        Some(variant.span),
                    );
                } else {
                    let parameters = (0..variant.fields.len())
                        .map(|index| format!("a{index}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    self.writer.line(
                        format!(
                            "function{} s{}({parameters}){{work({}, {}, {});return {{__enum:{},__variant:{},fields:[{parameters}]}};}}",
                            if self.mode == CompileMode::Debug { "*" } else { "" },
                            variant.symbol.0,
                            self.work.allocation,
                            variant.span.start,
                            variant.span.end,
                            enumeration.symbol.0,
                            variant.symbol.0
                        ),
                        Some(variant.span),
                    );
                }
            }
        }
    }

    fn generate_routines(&mut self) {
        for routine in self
            .ir
            .routines
            .iter()
            .filter(|routine| routine.kind != RoutineKind::Task)
        {
            let name = routine.symbol.map_or_else(
                || callback_name(routine.kind).to_owned(),
                |symbol| format!("s{}", symbol.0),
            );
            let parameters = routine
                .parameters
                .iter()
                .map(|symbol| format!("s{}", symbol.0))
                .collect::<Vec<_>>()
                .join(",");
            self.writer.line(
                format!(
                    "function{} {name}({parameters}){{",
                    if self.mode == CompileMode::Debug {
                        "*"
                    } else {
                        ""
                    }
                ),
                Some(routine.span),
            );
            self.writer.indent += 1;
            self.writer.line(
                format!(
                    "work({}, {}, {});",
                    self.work.function_entry, routine.span.start, routine.span.end
                ),
                Some(routine.span),
            );
            if self.mode == CompileMode::Debug {
                self.writer.line(
                    format!(
                        "enter({}, {});",
                        js_string(&routine.name),
                        self.source_span_js(routine.span)
                    ),
                    Some(routine.span),
                );
                self.writer.line("try{", Some(routine.span));
                self.writer.indent += 1;
            }
            let debug_locals_base = self.debug_locals.len();
            self.debug_locals.extend(&routine.parameters);
            self.generate_statements(&routine.body, ValueContext::Routine);
            self.debug_locals.truncate(debug_locals_base);
            if self.mode == CompileMode::Debug {
                self.writer.indent -= 1;
                self.writer.line("}finally{leave();}", Some(routine.span));
            }
            self.writer.indent -= 1;
            self.writer.line("}", Some(routine.span));
        }
    }

    fn generate_globals(&mut self) {
        self.writer.line(
            format!(
                "function{} initialize(){{",
                if self.mode == CompileMode::Debug {
                    "*"
                } else {
                    ""
                }
            ),
            None,
        );
        self.writer.indent += 1;
        for global in &self.ir.globals {
            self.generate_probe(global.span, ValueContext::Routine);
            let initializer = self.expression(&global.initializer, ValueContext::Routine);
            self.writer.line(
                format!("state.s{}={initializer};", global.symbol.0),
                Some(global.span),
            );
        }
        self.writer.indent -= 1;
        self.writer.line("}", None);
    }

    fn generate_tasks(&mut self) {
        for program in self.task_programs.clone() {
            let routine = self
                .ir
                .routines
                .iter()
                .find(|routine| routine.symbol == Some(program.symbol))
                .expect("task program retains routine symbol");
            self.generate_task_constructor(routine);
            self.generate_task_step(&program, routine);
        }
        self.writer
            .line("const taskFactories=Object.create(null);", None);
        self.writer
            .line("const taskSteppers=Object.create(null);", None);
        for program in &self.task_programs {
            self.writer.line(
                format!(
                    "taskFactories.s{}=newTask_s{};taskSteppers.s{}=stepTask_s{};",
                    program.symbol.0, program.symbol.0, program.symbol.0, program.symbol.0
                ),
                None,
            );
        }
        self.writer.line(
            "const startTask=(kind,args)=>{const factory=taskFactories[`s${kind}`];if(!factory)return api.fault(\"M019004\",\"unknown task\",{start:0,end:0});const task=factory(...args);tasks.push(task);return task.id;};",
            None,
        );
    }

    fn generate_task_constructor(&mut self, routine: &IrRoutine) {
        let symbol = routine.symbol.expect("task has a symbol");
        let parameters = routine
            .parameters
            .iter()
            .map(|parameter| format!("a{}", parameter.0))
            .collect::<Vec<_>>()
            .join(",");
        self.writer.line(
            format!("function newTask_s{}({parameters}){{", symbol.0),
            Some(routine.span),
        );
        self.writer.indent += 1;
        self.writer.line(
            format!(
                "work({}, {}, {});",
                self.work.allocation, routine.span.start, routine.span.end
            ),
            Some(routine.span),
        );
        self.writer.line(
            format!(
                "const task={{id:nextTaskId++,kind:{},pc:0,wait:0,done:false,locals:Object.create(null),iterators:Object.create(null),temps:Object.create(null)}};",
                symbol.0
            ),
            Some(routine.span),
        );
        for parameter in &routine.parameters {
            self.writer.line(
                format!("task.locals.s{}=a{};", parameter.0, parameter.0),
                Some(routine.span),
            );
        }
        self.writer.line("return task;", Some(routine.span));
        self.writer.indent -= 1;
        self.writer.line("}", Some(routine.span));
    }

    fn generate_task_step(&mut self, program: &TaskProgram, routine: &IrRoutine) {
        self.writer.line(
            format!(
                "function{} stepTask_s{}(task){{",
                if self.mode == CompileMode::Debug {
                    "*"
                } else {
                    ""
                },
                program.symbol.0
            ),
            Some(routine.span),
        );
        self.writer.indent += 1;
        if self.mode == CompileMode::Debug {
            self.writer.line(
                format!(
                    "enter({}, {});",
                    js_string(&routine.name),
                    self.source_span_js(routine.span)
                ),
                Some(routine.span),
            );
            self.writer.line("try{", Some(routine.span));
            self.writer.indent += 1;
        }
        self.writer.line("while(!task.done){", Some(routine.span));
        self.writer.indent += 1;
        self.writer.line("switch(task.pc){", Some(routine.span));
        self.writer.indent += 1;
        for (index, instruction) in program.instructions.iter().enumerate() {
            self.writer
                .line(format!("case {index}:{{"), Some(instruction.span()));
            self.writer.indent += 1;
            self.writer.line(
                format!(
                    "work({}, {}, {});",
                    self.work.task_transition,
                    instruction.span().start,
                    instruction.span().end
                ),
                Some(instruction.span()),
            );
            self.generate_probe(instruction.span(), ValueContext::Task);
            self.generate_task_instruction(instruction, index + 1);
            self.writer.indent -= 1;
            self.writer.line("}", Some(instruction.span()));
        }
        self.writer
            .line("default:task.done=true;return;", Some(routine.span));
        self.writer.indent -= 1;
        self.writer.line("}", Some(routine.span));
        self.writer.indent -= 1;
        self.writer.line("}", Some(routine.span));
        if self.mode == CompileMode::Debug {
            self.writer.indent -= 1;
            self.writer.line("}finally{leave();}", Some(routine.span));
        }
        self.writer.indent -= 1;
        self.writer.line("}", Some(routine.span));
    }

    #[allow(clippy::too_many_lines)]
    fn generate_task_instruction(&mut self, instruction: &TaskInstruction, next: usize) {
        let span = instruction.span();
        match instruction {
            TaskInstruction::Let { symbol, value, .. } => {
                let value = self.expression(value, ValueContext::Task);
                self.writer.line(
                    format!("task.locals.s{}={value};task.pc={next};continue;", symbol.0),
                    Some(span),
                );
            }
            TaskInstruction::Store {
                target,
                operator,
                value,
                ..
            } => {
                let assignment =
                    self.store_statement(target, *operator, value, ValueContext::Task, span);
                self.writer
                    .line(format!("{assignment};task.pc={next};continue;"), Some(span));
            }
            TaskInstruction::Expression { value, .. } => {
                let value = self.expression(value, ValueContext::Task);
                self.writer
                    .line(format!("{value};task.pc={next};continue;"), Some(span));
            }
            TaskInstruction::Start { value, .. } => {
                let value = self.start_expression(value, ValueContext::Task);
                self.writer
                    .line(format!("{value};task.pc={next};continue;"), Some(span));
            }
            TaskInstruction::Assert {
                condition, message, ..
            } => {
                let condition = self.expression(condition, ValueContext::Task);
                let message = message.as_ref().map_or_else(
                    || js_string("assertion failed"),
                    |message| self.expression(message, ValueContext::Task),
                );
                self.writer.line(
                    format!(
                        "if(!({condition}))api.fault(\"M019005\",{message},{{start:{},end:{}}});task.pc={next};continue;",
                        span.start, span.end
                    ),
                    Some(span),
                );
            }
            TaskInstruction::Wait { duration, .. } => {
                let duration = self.expression(duration, ValueContext::Task);
                self.writer.line(
                    format!(
                        "task.wait=Math.max(0,integer({duration},{},{}));task.pc={next};return;",
                        span.start, span.end
                    ),
                    Some(span),
                );
            }
            TaskInstruction::Jump { target, .. } => {
                self.writer
                    .line(format!("task.pc={target};continue;"), Some(span));
            }
            TaskInstruction::JumpIfFalse {
                condition, target, ..
            } => {
                let condition = self.expression(condition, ValueContext::Task);
                self.writer.line(
                    format!(
                        "if(!({condition})){{task.pc={target};continue;}}task.pc={next};continue;"
                    ),
                    Some(span),
                );
            }
            TaskInstruction::ForInit {
                iterator,
                iterable: value,
                ..
            } => {
                let value = self.expression(value, ValueContext::Task);
                self.writer.line(
                    format!(
                        "task.iterators.i{iterator}={{values:iterable({value},{},{}),index:0}};task.pc={next};continue;",
                        span.start, span.end
                    ),
                    Some(span),
                );
            }
            TaskInstruction::ForNext {
                iterator,
                binding,
                end,
                ..
            } => {
                self.writer.line(
                    format!(
                        "const iterator=task.iterators.i{iterator};if(iterator.index>=iterator.values.length){{delete task.iterators.i{iterator};task.pc={end};continue;}}task.locals.s{}=iterator.values[iterator.index++];task.pc={next};continue;",
                        binding.0
                    ),
                    Some(span),
                );
            }
            TaskInstruction::MatchValue {
                temporary, value, ..
            } => {
                let value = self.expression(value, ValueContext::Task);
                self.writer.line(
                    format!("task.temps.t{temporary}={value};task.pc={next};continue;"),
                    Some(span),
                );
            }
            TaskInstruction::Pattern {
                temporary,
                pattern,
                fail,
                ..
            } => {
                let subject = format!("task.temps.t{temporary}");
                let condition = pattern_condition(pattern, &subject);
                let bindings = pattern_bindings(pattern, &subject, ValueContext::Task);
                self.writer.line(
                    format!(
                        "if(!({condition})){{task.pc={fail};continue;}}{bindings}task.pc={next};continue;"
                    ),
                    Some(span),
                );
            }
            TaskInstruction::Return { value, .. } => {
                if let Some(value) = value {
                    let value = self.expression(value, ValueContext::Task);
                    self.writer.line(format!("void ({value});"), Some(span));
                }
                self.writer.line("task.done=true;return;", Some(span));
            }
        }
    }

    fn generate_scheduler(&mut self) {
        if self.mode == CompileMode::Debug {
            self.writer.line(
                "function* stepTasks(){for(const task of tasks){if(task.done)continue;if(task.wait>0){task.wait-=1;continue;}yield* taskSteppers[`s${task.kind}`](task);}for(let index=tasks.length-1;index>=0;index-=1){if(tasks[index].done)tasks.splice(index,1);}}",
                None,
            );
        } else {
            self.writer.line(
                "const stepTasks=()=>{for(const task of tasks){if(task.done)continue;if(task.wait>0){task.wait-=1;continue;}taskSteppers[`s${task.kind}`](task);}for(let index=tasks.length-1;index>=0;index-=1){if(tasks[index].done)tasks.splice(index,1);}};",
                None,
            );
        }
    }

    fn generate_instance(&mut self) {
        let has_start = self.has_callback(RoutineKind::Callback(crate::ast::CallbackKind::Start));
        let has_update = self.has_callback(RoutineKind::Callback(crate::ast::CallbackKind::Update));
        let has_draw = self.has_callback(RoutineKind::Callback(crate::ast::CallbackKind::Draw));
        let has_raster = self.has_callback(RoutineKind::Callback(crate::ast::CallbackKind::Raster));
        if self.mode == CompileMode::Debug {
            self.generate_debug_instance();
            return;
        }
        self.writer.line("return Object.freeze({", None);
        self.writer.indent += 1;
        self.writer.line(
            format!(
                "start:()=>{{initialize();{}{}}},",
                if has_start { "cb_start();" } else { "" },
                "stepTasks();"
            ),
            None,
        );
        self.writer.line(
            format!(
                "update:()=>{{{}stepTasks();}},",
                if has_update { "cb_update();" } else { "" }
            ),
            None,
        );
        self.writer.line(
            format!("draw:()=>{{{}}},", if has_draw { "cb_draw();" } else { "" }),
            None,
        );
        self.writer.line(
            format!(
                "raster:line=>{{{}}},",
                if has_raster {
                    "cb_raster(line);"
                } else {
                    "void line;"
                }
            ),
            None,
        );
        self.writer.line(
            "snapshot:()=>({state:structuredClone(state),tasks:structuredClone(tasks),nextTaskId}),",
            None,
        );
        self.writer.line(
            "restore:snapshot=>{for(const key of Object.keys(state))delete state[key];Object.assign(state,structuredClone(snapshot.state));tasks.splice(0,tasks.length,...structuredClone(snapshot.tasks));nextTaskId=snapshot.nextTaskId;},",
            None,
        );
        self.writer
            .line("inspect:()=>({state,tasks,callStack:[...callStack]})", None);
        self.writer.indent -= 1;
        self.writer.line("});", None);
    }

    fn generate_debug_instance(&mut self) {
        let has_start = self.has_callback(RoutineKind::Callback(crate::ast::CallbackKind::Start));
        let has_update = self.has_callback(RoutineKind::Callback(crate::ast::CallbackKind::Update));
        let has_draw = self.has_callback(RoutineKind::Callback(crate::ast::CallbackKind::Draw));
        let has_raster = self.has_callback(RoutineKind::Callback(crate::ast::CallbackKind::Raster));
        self.writer
            .line("const debugExecutions=Object.create(null);", None);
        self.writer.line(
            "const advance=(name,factory,args=[])=>{let execution=debugExecutions[name];if(execution===undefined){execution=factory(...args);debugExecutions[name]=execution;}const step=execution.next();if(step.done)delete debugExecutions[name];return step.done?{done:true}:{done:false,event:step.value};};",
            None,
        );
        self.writer.line("function* debug_start(){", None);
        self.writer.indent += 1;
        self.writer.line("yield* initialize();", None);
        if has_start {
            self.writer.line("yield* cb_start();", None);
        }
        self.writer.line("yield* stepTasks();", None);
        self.writer.indent -= 1;
        self.writer.line("}", None);
        self.writer.line("function* debug_update(){", None);
        self.writer.indent += 1;
        if has_update {
            self.writer.line("yield* cb_update();", None);
        }
        self.writer.line("yield* stepTasks();", None);
        self.writer.indent -= 1;
        self.writer.line("}", None);
        self.writer.line("function* debug_draw(){", None);
        self.writer.indent += 1;
        if has_draw {
            self.writer.line("yield* cb_draw();", None);
        }
        self.writer.indent -= 1;
        self.writer.line("}", None);
        self.writer.line("function* debug_raster(line){", None);
        self.writer.indent += 1;
        if has_raster {
            self.writer.line("yield* cb_raster(line);", None);
        } else {
            self.writer.line("void line;", None);
        }
        self.writer.indent -= 1;
        self.writer.line("}", None);
        self.writer.line("return Object.freeze({", None);
        self.writer.indent += 1;
        self.writer
            .line("start:()=>advance('start',debug_start),", None);
        self.writer
            .line("update:()=>advance('update',debug_update),", None);
        self.writer
            .line("draw:()=>advance('draw',debug_draw),", None);
        self.writer
            .line("raster:line=>advance('raster',debug_raster,[line]),", None);
        self.writer.line(
            "snapshot:()=>({state:structuredClone(state),tasks:structuredClone(tasks),nextTaskId}),",
            None,
        );
        self.writer.line(
            "restore:snapshot=>{for(const key of Object.keys(debugExecutions))delete debugExecutions[key];for(const key of Object.keys(state))delete state[key];Object.assign(state,structuredClone(snapshot.state));tasks.splice(0,tasks.length,...structuredClone(snapshot.tasks));nextTaskId=snapshot.nextTaskId;},",
            None,
        );
        self.writer
            .line("inspect:()=>({state,tasks,callStack:[...callStack]})", None);
        self.writer.indent -= 1;
        self.writer.line("});", None);
    }

    fn has_callback(&self, kind: RoutineKind) -> bool {
        self.ir.routines.iter().any(|routine| routine.kind == kind)
    }

    fn map_span(&self, span: Span) -> Span {
        self.origins.map_or(span, |origins| origins.map_span(span))
    }

    fn source_span_js(&self, span: Span) -> String {
        let span = self.map_span(span);
        let source = self
            .origins
            .and_then(|origins| origins.source(span.file))
            .unwrap_or(self.source);
        format!(
            "{{source:{},start:{},end:{}}}",
            js_string(source.name()),
            span.start,
            span.end
        )
    }

    fn generate_statements(&mut self, statements: &[IrStatement], context: ValueContext) {
        let debug_locals_base = self.debug_locals.len();
        for statement in statements {
            self.generate_probe(statement.span, context);
            self.generate_statement(statement, context);
            if let IrStatementKind::Let { symbol, .. } = &statement.kind {
                self.debug_locals.push(*symbol);
            }
        }
        self.debug_locals.truncate(debug_locals_base);
    }

    #[allow(clippy::too_many_lines)]
    fn generate_statement(&mut self, statement: &IrStatement, context: ValueContext) {
        match &statement.kind {
            IrStatementKind::Let {
                symbol,
                mutable,
                value,
            } => {
                let keyword = if *mutable { "let" } else { "const" };
                let value = self.expression(value, context);
                self.writer.line(
                    format!("{keyword} s{}={value};", symbol.0),
                    Some(statement.span),
                );
            }
            IrStatementKind::Store {
                target,
                operator,
                value,
            } => {
                let assignment =
                    self.store_statement(target, *operator, value, context, statement.span);
                self.writer
                    .line(format!("{assignment};"), Some(statement.span));
            }
            IrStatementKind::Expression(value) => {
                let value = self.expression(value, context);
                self.writer.line(format!("{value};"), Some(statement.span));
            }
            IrStatementKind::Return(value) => {
                let value = value.as_ref().map_or_else(
                    || "undefined".to_owned(),
                    |value| self.expression(value, context),
                );
                self.writer
                    .line(format!("return {value};"), Some(statement.span));
            }
            IrStatementKind::Break => self.writer.line("break;", Some(statement.span)),
            IrStatementKind::Continue => self.writer.line("continue;", Some(statement.span)),
            IrStatementKind::Wait(_) => self.writer.line(
                "api.fault(\"M019006\",\"wait escaped task lowering\",{start:0,end:0});",
                Some(statement.span),
            ),
            IrStatementKind::Start(value) => {
                let value = self.start_expression(value, context);
                self.writer.line(format!("{value};"), Some(statement.span));
            }
            IrStatementKind::Assert { condition, message } => {
                let condition = self.expression(condition, context);
                let message = message.as_ref().map_or_else(
                    || js_string("assertion failed"),
                    |message| self.expression(message, context),
                );
                self.writer.line(
                    format!(
                        "if(!({condition}))api.fault(\"M019005\",{message},{{start:{},end:{}}});",
                        statement.span.start, statement.span.end
                    ),
                    Some(statement.span),
                );
            }
            IrStatementKind::If {
                branches,
                else_body,
            } => self.generate_if(branches, else_body.as_deref(), context),
            IrStatementKind::While { condition, body } => {
                let condition = self.expression(condition, context);
                self.writer
                    .line(format!("while({condition}){{"), Some(statement.span));
                self.writer.indent += 1;
                self.writer.line(
                    format!(
                        "work({}, {}, {});",
                        self.work.loop_back_edge, statement.span.start, statement.span.end
                    ),
                    Some(statement.span),
                );
                self.generate_statements(body, context);
                self.writer.indent -= 1;
                self.writer.line("}", Some(statement.span));
            }
            IrStatementKind::For {
                binding,
                iterable: value,
                body,
            } => {
                let value = self.expression(value, context);
                self.writer.line(
                    format!(
                        "for(const s{} of iterable({value},{},{})){{",
                        binding.0, statement.span.start, statement.span.end
                    ),
                    Some(statement.span),
                );
                self.writer.indent += 1;
                self.writer.line(
                    format!(
                        "work({}, {}, {});",
                        self.work.loop_back_edge, statement.span.start, statement.span.end
                    ),
                    Some(statement.span),
                );
                let debug_locals_base = self.debug_locals.len();
                self.debug_locals.push(*binding);
                self.generate_statements(body, context);
                self.debug_locals.truncate(debug_locals_base);
                self.writer.indent -= 1;
                self.writer.line("}", Some(statement.span));
            }
            IrStatementKind::Match { subject, arms } => {
                self.generate_match(subject, arms, context, statement.span);
            }
        }
    }

    fn generate_if(
        &mut self,
        branches: &[IrBranch],
        else_body: Option<&[IrStatement]>,
        context: ValueContext,
    ) {
        for (index, branch) in branches.iter().enumerate() {
            let condition = self.expression(&branch.condition, context);
            self.writer.line(
                format!(
                    "{}({condition}){{",
                    if index == 0 { "if" } else { "else if" }
                ),
                Some(branch.span),
            );
            self.writer.indent += 1;
            self.generate_statements(&branch.body, context);
            self.writer.indent -= 1;
            self.writer.line("}", Some(branch.span));
        }
        if let Some(body) = else_body {
            self.writer.line("else{", None);
            self.writer.indent += 1;
            self.generate_statements(body, context);
            self.writer.indent -= 1;
            self.writer.line("}", None);
        }
    }

    fn generate_match(
        &mut self,
        subject: &IrExpression,
        arms: &[IrMatchArm],
        context: ValueContext,
        span: Span,
    ) {
        let temporary = self.next_temporary();
        let subject_value = self.expression(subject, context);
        self.writer.line("{", Some(span));
        self.writer.indent += 1;
        self.writer.line(
            format!("const t{temporary}={subject_value};"),
            Some(subject.span),
        );
        for (index, arm) in arms.iter().enumerate() {
            let condition = pattern_condition(&arm.pattern, &format!("t{temporary}"));
            self.writer.line(
                format!(
                    "{}({condition}){{",
                    if index == 0 { "if" } else { "else if" }
                ),
                Some(arm.span),
            );
            self.writer.indent += 1;
            let bindings = pattern_bindings(&arm.pattern, &format!("t{temporary}"), context);
            if !bindings.is_empty() {
                self.writer.line(bindings, Some(arm.span));
            }
            let debug_locals_base = self.debug_locals.len();
            self.debug_locals.extend(pattern_symbols(&arm.pattern));
            self.generate_statements(&arm.body, context);
            self.debug_locals.truncate(debug_locals_base);
            self.writer.indent -= 1;
            self.writer.line("}", Some(arm.span));
        }
        self.writer.indent -= 1;
        self.writer.line("}", Some(span));
    }

    fn generate_probe(&mut self, span: Span, context: ValueContext) {
        if self.mode != CompileMode::Debug {
            return;
        }
        let id = self.probe_count;
        self.probe_count += 1;
        let locals = match context {
            ValueContext::Routine => format!(
                "{{{}}}",
                self.debug_locals
                    .iter()
                    .map(|symbol| format!("s{}:s{}", symbol.0, symbol.0))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            ValueContext::Task => "task.locals".to_owned(),
        };
        self.writer.line(
            format!(
                "yield {{id:{id},sourceSpan:{},locals:{locals}}};",
                self.source_span_js(span)
            ),
            Some(span),
        );
    }

    fn start_expression(&mut self, expression: &IrExpression, context: ValueContext) -> String {
        if let IrExpressionKind::Call { callee, arguments } = &expression.kind
            && self.symbol(*callee).kind == SymbolKind::Task
        {
            let arguments = arguments
                .iter()
                .map(|argument| self.expression(argument, context))
                .collect::<Vec<_>>()
                .join(",");
            return format!("startTask({},[{arguments}])", callee.0);
        }
        self.expression(expression, context)
    }

    #[allow(clippy::too_many_lines)]
    fn expression(&mut self, expression: &IrExpression, context: ValueContext) -> String {
        match &expression.kind {
            IrExpressionKind::Literal(literal) => literal_js(literal),
            IrExpressionKind::Load(symbol) => self.load(*symbol, context),
            IrExpressionKind::Asset { name, kind } => format!(
                "Object.freeze({{name:{},kind:{}}})",
                js_string(name),
                js_string(kind.type_name())
            ),
            IrExpressionKind::Array(elements) => format!(
                "allocate([{}],{},{})",
                elements
                    .iter()
                    .map(|element| self.expression(element, context))
                    .collect::<Vec<_>>()
                    .join(","),
                expression.span.start,
                expression.span.end
            ),
            IrExpressionKind::Unary { operator, operand } => {
                let operand = self.expression(operand, context);
                let operator = match operator {
                    UnaryOperator::Negate => "-",
                    UnaryOperator::Positive => "+",
                    UnaryOperator::Not => "!",
                };
                format!("({operator}{operand})")
            }
            IrExpressionKind::Binary {
                left,
                operator,
                right,
            } => {
                let enum_comparison = matches!(left.r#type, Type::Enum(_))
                    && matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual);
                let left = self.expression(left, context);
                let right = self.expression(right, context);
                if enum_comparison {
                    let negate = if *operator == BinaryOperator::NotEqual {
                        "!"
                    } else {
                        ""
                    };
                    return format!(
                        "{negate}enumEqual({left},{right},{},{})",
                        expression.span.start, expression.span.end
                    );
                }
                if expression.r#type == Type::Range {
                    return format!("({{__range:true,start:{left},end:{right}}})");
                }
                if expression.r#type == Type::Int {
                    match operator {
                        BinaryOperator::Divide => {
                            return format!(
                                "divideInt({left},{right},{},{})",
                                expression.span.start, expression.span.end
                            );
                        }
                        BinaryOperator::Add
                        | BinaryOperator::Subtract
                        | BinaryOperator::Multiply
                        | BinaryOperator::Remainder => {
                            return format!(
                                "integer(({left}){}({right}),{},{})",
                                binary_operator(*operator),
                                expression.span.start,
                                expression.span.end
                            );
                        }
                        _ => {}
                    }
                }
                format!("(({left}){}({right}))", binary_operator(*operator))
            }
            IrExpressionKind::Call { callee, arguments } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expression(argument, context))
                    .collect::<Vec<_>>()
                    .join(",");
                let symbol = self.symbol(*callee);
                match symbol.kind {
                    SymbolKind::Builtin => match symbol.name.as_str() {
                        "some" => format!(
                            "optionSome({arguments},{},{})",
                            expression.span.start, expression.span.end
                        ),
                        "is_some" => format!("optionIsSome({arguments})"),
                        "unwrap_or" => format!("optionUnwrap({arguments})"),
                        _ => format!(
                            "consoleCall({},[{arguments}],{},{})",
                            js_string(&symbol.name),
                            expression.span.start,
                            expression.span.end
                        ),
                    },
                    SymbolKind::Task => format!("startTask({},[{arguments}])", callee.0),
                    SymbolKind::Function | SymbolKind::Record | SymbolKind::Variant
                        if self.mode == CompileMode::Debug =>
                    {
                        format!("(yield* s{}({arguments}))", callee.0)
                    }
                    _ => format!("s{}({arguments})", callee.0),
                }
            }
            IrExpressionKind::Field { subject, field } => format!(
                "({}).{}",
                self.expression(subject, context),
                safe_property(field)
            ),
            IrExpressionKind::Index { subject, index } => {
                let subject_type = subject.r#type.clone();
                let subject = self.expression(subject, context);
                let index = self.expression(index, context);
                match subject_type {
                    Type::Array { length, .. } => format!(
                        "collectionRead(({subject}),({index}),{length},{},{})",
                        expression.span.start, expression.span.end
                    ),
                    Type::List { capacity, .. } => format!(
                        "collectionRead(({subject}),({index}),{capacity},{},{})",
                        expression.span.start, expression.span.end
                    ),
                    Type::Text => format!(
                        "textIndex(({subject}),({index}),{},{})",
                        expression.span.start, expression.span.end
                    ),
                    _ => format!("({subject})[{index}]"),
                }
            }
            IrExpressionKind::Error => "undefined".to_owned(),
        }
    }

    fn place(&mut self, place: &IrPlace, context: ValueContext) -> String {
        match place {
            IrPlace::Symbol(symbol) => self.load(*symbol, context),
            IrPlace::Field { subject, field } => format!(
                "({}).{}",
                self.expression(subject, context),
                safe_property(field)
            ),
            IrPlace::Index { subject, index } => format!(
                "({})[{}]",
                self.expression(subject, context),
                self.expression(index, context)
            ),
            IrPlace::Error => "undefined".to_owned(),
        }
    }

    fn store_statement(
        &mut self,
        place: &IrPlace,
        operator: AssignmentOperator,
        value: &IrExpression,
        context: ValueContext,
        span: Span,
    ) -> String {
        let value_js = self.expression(value, context);
        if let IrPlace::Index { subject, index } = place
            && let Some(limit) = collection_capacity(&subject.r#type)
        {
            let subject = self.expression(subject, context);
            let index = self.expression(index, context);
            let current = format!(
                "collectionRead(object,key,{limit},{},{})",
                span.start, span.end
            );
            let next = if operator == AssignmentOperator::Assign {
                "next".to_owned()
            } else if value.r#type == Type::Int {
                match operator {
                    AssignmentOperator::Divide => {
                        format!("divideInt({current},next,{},{})", span.start, span.end)
                    }
                    AssignmentOperator::Add
                    | AssignmentOperator::Subtract
                    | AssignmentOperator::Multiply => format!(
                        "integer(({current}){}(next),{},{})",
                        assignment_operator(operator).trim_end_matches('='),
                        span.start,
                        span.end
                    ),
                    AssignmentOperator::Assign => unreachable!("handled above"),
                }
            } else {
                format!(
                    "({current}){}(next)",
                    assignment_operator(operator).trim_end_matches('=')
                )
            };
            return format!(
                "((object,key,next)=>collectionWrite(object,key,{limit},{next},{},{}))(({subject}),({index}),({value_js}))",
                span.start, span.end
            );
        }
        if operator == AssignmentOperator::Assign || value.r#type != Type::Int {
            return format!(
                "{}{}{value_js}",
                self.place(place, context),
                assignment_operator(operator)
            );
        }
        let checked = |left: &str, value: &str| match operator {
            AssignmentOperator::Divide => {
                format!("divideInt({left},{value},{},{})", span.start, span.end)
            }
            AssignmentOperator::Add
            | AssignmentOperator::Subtract
            | AssignmentOperator::Multiply => format!(
                "integer(({left}){}({value}),{},{})",
                assignment_operator(operator).trim_end_matches('='),
                span.start,
                span.end
            ),
            AssignmentOperator::Assign => unreachable!("handled before integer compound store"),
        };
        match place {
            IrPlace::Symbol(symbol) => {
                let target = self.load(*symbol, context);
                format!("{target}={}", checked(&target, &value_js))
            }
            IrPlace::Field { subject, field } => {
                let subject = self.expression(subject, context);
                let property = safe_property(field);
                let left = format!("object.{property}");
                format!(
                    "((object,value)=>({left}={}))(({subject}),({value_js}))",
                    checked(&left, "value")
                )
            }
            IrPlace::Index { subject, index } => {
                let subject = self.expression(subject, context);
                let index = self.expression(index, context);
                format!(
                    "((object,key,value)=>(object[key]={}))(({subject}),({index}),({value_js}))",
                    checked("object[key]", "value")
                )
            }
            IrPlace::Error => "undefined".to_owned(),
        }
    }

    fn load(&self, symbol: SymbolId, context: ValueContext) -> String {
        let definition = self.symbol(symbol);
        match definition.kind {
            SymbolKind::Constant | SymbolKind::State => format!("state.s{}", symbol.0),
            SymbolKind::Parameter | SymbolKind::Local if context == ValueContext::Task => {
                format!("task.locals.s{}", symbol.0)
            }
            SymbolKind::Parameter | SymbolKind::Local => format!("s{}", symbol.0),
            SymbolKind::Function | SymbolKind::Task | SymbolKind::Record | SymbolKind::Variant => {
                format!("s{}", symbol.0)
            }
            SymbolKind::Builtin => builtin_value_js(&definition.name)
                .map_or_else(|| definition.name.clone(), ToOwned::to_owned),
            SymbolKind::Import | SymbolKind::Enum => "undefined".to_owned(),
        }
    }

    fn symbol(&self, id: SymbolId) -> &Symbol {
        &self.symbols[*self
            .symbol_index
            .get(&id)
            .expect("typed IR references a declared symbol")]
    }

    fn next_temporary(&mut self) -> u32 {
        let temporary = self.temporary;
        self.temporary += 1;
        temporary
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ValueContext {
    Routine,
    Task,
}

#[derive(Clone, Debug)]
struct TaskProgram {
    symbol: SymbolId,
    instructions: Vec<TaskInstruction>,
}

impl TaskProgram {
    fn lower(routine: &IrRoutine) -> Self {
        let mut lowerer = TaskLowerer {
            instructions: Vec::new(),
            loops: Vec::new(),
            iterator_count: 0,
            temporary_count: 0,
        };
        lowerer.lower_block(&routine.body);
        lowerer.instructions.push(TaskInstruction::Return {
            value: None,
            span: routine.span,
        });
        lowerer.patch_remaining(routine.span);
        Self {
            symbol: routine.symbol.expect("task routine has a symbol"),
            instructions: lowerer.instructions,
        }
    }
}

#[derive(Clone, Debug)]
enum TaskInstruction {
    Let {
        symbol: SymbolId,
        value: IrExpression,
        span: Span,
    },
    Store {
        target: IrPlace,
        operator: AssignmentOperator,
        value: IrExpression,
        span: Span,
    },
    Expression {
        value: IrExpression,
        span: Span,
    },
    Start {
        value: IrExpression,
        span: Span,
    },
    Assert {
        condition: IrExpression,
        message: Option<IrExpression>,
        span: Span,
    },
    Wait {
        duration: IrExpression,
        span: Span,
    },
    Jump {
        target: usize,
        span: Span,
    },
    JumpIfFalse {
        condition: IrExpression,
        target: usize,
        span: Span,
    },
    ForInit {
        iterator: u32,
        iterable: IrExpression,
        span: Span,
    },
    ForNext {
        iterator: u32,
        binding: SymbolId,
        end: usize,
        span: Span,
    },
    MatchValue {
        temporary: u32,
        value: IrExpression,
        span: Span,
    },
    Pattern {
        temporary: u32,
        pattern: IrPattern,
        fail: usize,
        span: Span,
    },
    Return {
        value: Option<IrExpression>,
        span: Span,
    },
}

impl TaskInstruction {
    const fn span(&self) -> Span {
        match self {
            Self::Let { span, .. }
            | Self::Store { span, .. }
            | Self::Expression { span, .. }
            | Self::Start { span, .. }
            | Self::Assert { span, .. }
            | Self::Wait { span, .. }
            | Self::Jump { span, .. }
            | Self::JumpIfFalse { span, .. }
            | Self::ForInit { span, .. }
            | Self::ForNext { span, .. }
            | Self::MatchValue { span, .. }
            | Self::Pattern { span, .. }
            | Self::Return { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug)]
struct LoopContext {
    continue_target: usize,
    breaks: Vec<usize>,
}

struct TaskLowerer {
    instructions: Vec<TaskInstruction>,
    loops: Vec<LoopContext>,
    iterator_count: u32,
    temporary_count: u32,
}

impl TaskLowerer {
    fn lower_block(&mut self, block: &[IrStatement]) {
        for statement in block {
            self.lower_statement(statement);
        }
    }

    #[allow(clippy::too_many_lines)]
    fn lower_statement(&mut self, statement: &IrStatement) {
        match &statement.kind {
            IrStatementKind::Let { symbol, value, .. } => {
                self.instructions.push(TaskInstruction::Let {
                    symbol: *symbol,
                    value: value.clone(),
                    span: statement.span,
                });
            }
            IrStatementKind::Store {
                target,
                operator,
                value,
            } => self.instructions.push(TaskInstruction::Store {
                target: target.clone(),
                operator: *operator,
                value: value.clone(),
                span: statement.span,
            }),
            IrStatementKind::Expression(value) => {
                self.instructions.push(TaskInstruction::Expression {
                    value: value.clone(),
                    span: statement.span,
                });
            }
            IrStatementKind::Return(value) => self.instructions.push(TaskInstruction::Return {
                value: value.clone(),
                span: statement.span,
            }),
            IrStatementKind::Break => {
                let index = self.instructions.len();
                self.instructions.push(TaskInstruction::Jump {
                    target: usize::MAX,
                    span: statement.span,
                });
                if let Some(context) = self.loops.last_mut() {
                    context.breaks.push(index);
                }
            }
            IrStatementKind::Continue => {
                let target = self
                    .loops
                    .last()
                    .map_or(self.instructions.len() + 1, |context| {
                        context.continue_target
                    });
                self.instructions.push(TaskInstruction::Jump {
                    target,
                    span: statement.span,
                });
            }
            IrStatementKind::Wait(duration) => self.instructions.push(TaskInstruction::Wait {
                duration: duration.clone(),
                span: statement.span,
            }),
            IrStatementKind::Start(value) => self.instructions.push(TaskInstruction::Start {
                value: value.clone(),
                span: statement.span,
            }),
            IrStatementKind::Assert { condition, message } => {
                self.instructions.push(TaskInstruction::Assert {
                    condition: condition.clone(),
                    message: message.clone(),
                    span: statement.span,
                });
            }
            IrStatementKind::If {
                branches,
                else_body,
            } => self.lower_if(branches, else_body.as_deref()),
            IrStatementKind::While { condition, body } => {
                let condition_index = self.instructions.len();
                self.instructions.push(TaskInstruction::JumpIfFalse {
                    condition: condition.clone(),
                    target: usize::MAX,
                    span: statement.span,
                });
                self.loops.push(LoopContext {
                    continue_target: condition_index,
                    breaks: Vec::new(),
                });
                self.lower_block(body);
                self.instructions.push(TaskInstruction::Jump {
                    target: condition_index,
                    span: statement.span,
                });
                let end = self.instructions.len();
                self.patch_target(condition_index, end);
                self.patch_loop_breaks(end);
            }
            IrStatementKind::For {
                binding,
                iterable,
                body,
            } => {
                let iterator = self.iterator_count;
                self.iterator_count += 1;
                self.instructions.push(TaskInstruction::ForInit {
                    iterator,
                    iterable: iterable.clone(),
                    span: statement.span,
                });
                let next_index = self.instructions.len();
                self.instructions.push(TaskInstruction::ForNext {
                    iterator,
                    binding: *binding,
                    end: usize::MAX,
                    span: statement.span,
                });
                self.loops.push(LoopContext {
                    continue_target: next_index,
                    breaks: Vec::new(),
                });
                self.lower_block(body);
                self.instructions.push(TaskInstruction::Jump {
                    target: next_index,
                    span: statement.span,
                });
                let end = self.instructions.len();
                self.patch_target(next_index, end);
                self.patch_loop_breaks(end);
            }
            IrStatementKind::Match { subject, arms } => {
                self.lower_match(subject, arms, statement.span);
            }
        }
    }

    fn lower_if(&mut self, branches: &[IrBranch], else_body: Option<&[IrStatement]>) {
        let mut end_jumps = Vec::new();
        for branch in branches {
            let condition_index = self.instructions.len();
            self.instructions.push(TaskInstruction::JumpIfFalse {
                condition: branch.condition.clone(),
                target: usize::MAX,
                span: branch.span,
            });
            self.lower_block(&branch.body);
            let end_jump = self.instructions.len();
            self.instructions.push(TaskInstruction::Jump {
                target: usize::MAX,
                span: branch.span,
            });
            end_jumps.push(end_jump);
            let next = self.instructions.len();
            self.patch_target(condition_index, next);
        }
        if let Some(body) = else_body {
            self.lower_block(body);
        }
        let end = self.instructions.len();
        for jump in end_jumps {
            self.patch_target(jump, end);
        }
    }

    fn lower_match(&mut self, subject: &IrExpression, arms: &[IrMatchArm], span: Span) {
        let temporary = self.temporary_count;
        self.temporary_count += 1;
        self.instructions.push(TaskInstruction::MatchValue {
            temporary,
            value: subject.clone(),
            span,
        });
        let mut end_jumps = Vec::new();
        for arm in arms {
            let pattern_index = self.instructions.len();
            self.instructions.push(TaskInstruction::Pattern {
                temporary,
                pattern: arm.pattern.clone(),
                fail: usize::MAX,
                span: arm.span,
            });
            self.lower_block(&arm.body);
            let end_jump = self.instructions.len();
            self.instructions.push(TaskInstruction::Jump {
                target: usize::MAX,
                span: arm.span,
            });
            end_jumps.push(end_jump);
            let next = self.instructions.len();
            self.patch_target(pattern_index, next);
        }
        let end = self.instructions.len();
        for jump in end_jumps {
            self.patch_target(jump, end);
        }
    }

    fn patch_target(&mut self, index: usize, target: usize) {
        match &mut self.instructions[index] {
            TaskInstruction::Jump {
                target: destination,
                ..
            }
            | TaskInstruction::JumpIfFalse {
                target: destination,
                ..
            } => *destination = target,
            TaskInstruction::ForNext { end, .. } => *end = target,
            TaskInstruction::Pattern { fail, .. } => *fail = target,
            _ => unreachable!("only branch instructions are patched"),
        }
    }

    fn patch_loop_breaks(&mut self, target: usize) {
        let context = self.loops.pop().expect("loop context exists");
        for index in context.breaks {
            self.patch_target(index, target);
        }
    }

    fn patch_remaining(&mut self, span: Span) {
        let end = self.instructions.len();
        for index in 0..self.instructions.len() {
            let unpatched = match &self.instructions[index] {
                TaskInstruction::Jump { target, .. }
                | TaskInstruction::JumpIfFalse { target, .. } => *target == usize::MAX,
                TaskInstruction::ForNext { end, .. } => *end == usize::MAX,
                TaskInstruction::Pattern { fail, .. } => *fail == usize::MAX,
                _ => false,
            };
            if unpatched {
                self.patch_target(index, end);
            }
        }
        debug_assert!(self.loops.is_empty(), "all task loops close at {span:?}");
    }
}

#[derive(Default)]
struct Writer {
    output: String,
    indent: usize,
    line: u32,
    relationships: Vec<SourceRelationship>,
}

impl Writer {
    fn line(&mut self, text: impl AsRef<str>, source_span: Option<Span>) {
        for _ in 0..self.indent {
            self.output.push_str("  ");
        }
        self.output.push_str(text.as_ref());
        self.output.push('\n');
        if let Some(source_span) = source_span {
            self.relationships.push(SourceRelationship {
                generated_line: self.line,
                generated_column: 0,
                source_span,
            });
        }
        self.line += 1;
    }

    fn standard_mappings(&self, source: &SourceFile, origins: Option<&SourceOriginMap>) -> String {
        let by_line: BTreeMap<_, _> = self
            .relationships
            .iter()
            .map(|relationship| (relationship.generated_line, relationship.source_span))
            .collect();
        let mut mappings = String::new();
        let mut previous_source = 0_i64;
        let mut previous_original_line = 0_i64;
        let mut previous_original_column = 0_i64;
        for line in 0..self.line {
            if line > 0 {
                mappings.push(';');
            }
            let Some(span) = by_line.get(&line) else {
                continue;
            };
            let span = origins.map_or(*span, |origins| origins.map_span(*span));
            let original_source = origins
                .and_then(|origins| origins.source(span.file))
                .unwrap_or(source);
            let source_index = origins
                .and_then(|origins| {
                    origins
                        .sources
                        .iter()
                        .position(|candidate| candidate.id() == span.file)
                })
                .unwrap_or(0);
            let LineColumn {
                line: original_line,
                column: original_column,
            } = original_source.line_column(span.start);
            let segment = [
                0_i64,
                i64::try_from(source_index).unwrap_or(i64::MAX) - previous_source,
                i64::from(original_line) - previous_original_line,
                i64::from(original_column) - previous_original_column,
            ];
            for value in segment {
                mappings.push_str(&base64_vlq(value));
            }
            previous_source = i64::try_from(source_index).unwrap_or(i64::MAX);
            previous_original_line = i64::from(original_line);
            previous_original_column = i64::from(original_column);
        }
        mappings
    }
}

fn callback_name(kind: RoutineKind) -> &'static str {
    match kind {
        RoutineKind::Callback(crate::ast::CallbackKind::Start) => "cb_start",
        RoutineKind::Callback(crate::ast::CallbackKind::Update) => "cb_update",
        RoutineKind::Callback(crate::ast::CallbackKind::Draw) => "cb_draw",
        RoutineKind::Callback(crate::ast::CallbackKind::Raster) => "cb_raster",
        RoutineKind::Function | RoutineKind::Task => unreachable!("not a callback"),
    }
}

fn literal_js(literal: &Literal) -> String {
    match literal {
        Literal::Int(value) => value.to_string(),
        Literal::Num(value) => {
            let value = value.to_string();
            if value.contains('.') {
                value
            } else {
                format!("{value}.0")
            }
        }
        Literal::Bool(value) => value.to_string(),
        Literal::Text(value) => js_string(value),
        Literal::None => "null".to_owned(),
        Literal::Frames(value) => value.to_string(),
        Literal::Seconds(value) => {
            let frames = (*value * 60.0).round();
            format!("{frames:.0}")
        }
    }
}

const fn assignment_operator(operator: AssignmentOperator) -> &'static str {
    match operator {
        AssignmentOperator::Assign => "=",
        AssignmentOperator::Add => "+=",
        AssignmentOperator::Subtract => "-=",
        AssignmentOperator::Multiply => "*=",
        AssignmentOperator::Divide => "/=",
    }
}

const fn binary_operator(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Or => "||",
        BinaryOperator::And => "&&",
        BinaryOperator::Equal => "===",
        BinaryOperator::NotEqual => "!==",
        BinaryOperator::Less => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::Greater => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::Range => "..",
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Remainder => "%",
    }
}

fn pattern_condition(pattern: &IrPattern, subject: &str) -> String {
    match pattern {
        IrPattern::Wildcard | IrPattern::Binding(_) => "true".to_owned(),
        IrPattern::Variant { symbol, .. } => format!("{subject}?.__variant==={}", symbol.0),
        IrPattern::Literal(literal) => format!("{subject}==={}", literal_js(literal)),
        IrPattern::Error => "false".to_owned(),
    }
}

fn pattern_bindings(pattern: &IrPattern, subject: &str, context: ValueContext) -> String {
    match pattern {
        IrPattern::Binding(symbol) => match context {
            ValueContext::Routine => format!("const s{}={subject};", symbol.0),
            ValueContext::Task => format!("task.locals.s{}={subject};", symbol.0),
        },
        IrPattern::Variant { bindings, .. } => bindings
            .iter()
            .enumerate()
            .map(|(index, symbol)| match context {
                ValueContext::Routine => format!("const s{}={subject}.fields[{index}];", symbol.0),
                ValueContext::Task => {
                    format!("task.locals.s{}={subject}.fields[{index}];", symbol.0)
                }
            })
            .collect(),
        IrPattern::Wildcard | IrPattern::Literal(_) | IrPattern::Error => String::new(),
    }
}

fn pattern_symbols(pattern: &IrPattern) -> Vec<SymbolId> {
    match pattern {
        IrPattern::Binding(symbol) => vec![*symbol],
        IrPattern::Variant { bindings, .. } => bindings.clone(),
        IrPattern::Wildcard | IrPattern::Literal(_) | IrPattern::Error => Vec::new(),
    }
}

fn collection_capacity(r#type: &Type) -> Option<u32> {
    match r#type {
        Type::Array { length, .. } => Some(*length),
        Type::List { capacity, .. } => Some(*capacity),
        _ => None,
    }
}

fn safe_property(name: &str) -> String {
    if name.bytes().enumerate().all(|(index, byte)| {
        byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
    }) {
        name.to_owned()
    } else {
        format!("[{}]", js_string(name))
    }
}

fn js_string(value: &str) -> String {
    serde_json::to_string(value).expect("Rust strings always encode as JSON strings")
}

fn builtin_value_js(name: &str) -> Option<&'static str> {
    match name {
        "pad1" => Some("0"),
        "pad2" => Some("1"),
        "pad3" => Some("2"),
        "pad4" => Some("3"),
        "up" => Some("\"up\""),
        "down" => Some("\"down\""),
        "left" => Some("\"left\""),
        "right" => Some("\"right\""),
        "a" => Some("\"a\""),
        "b" => Some("\"b\""),
        "x" => Some("\"x\""),
        "y" => Some("\"y\""),
        "l" => Some("\"l\""),
        "r" => Some("\"r\""),
        "start_button" => Some("\"start\""),
        "menu" => Some("\"menu\""),
        _ => None,
    }
}

fn base64_vlq(value: i64) -> String {
    const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = if value < 0 {
        value.unsigned_abs() * 2 + 1
    } else {
        value.unsigned_abs() * 2
    };
    let mut output = String::new();
    loop {
        let mut digit = u8::try_from(encoded & 31).expect("VLQ digit is five bits");
        encoded >>= 5;
        if encoded > 0 {
            digit |= 32;
        }
        output.push(char::from(BASE64[usize::from(digit)]));
        if encoded == 0 {
            break;
        }
    }
    output
}

/// Returns only designed diagnostics from a failed compilation.
#[must_use]
pub fn compilation_diagnostics(output: &CompilationOutput) -> &[Diagnostic] {
    &output.analysis.diagnostics
}

#[cfg(test)]
mod tests {
    use crate::{
        AssetCatalog, CompileMode, FileId, SourceFile,
        codegen::{TaskInstruction, TaskProgram, base64_vlq, compile},
    };

    #[test]
    fn encodes_source_map_vlq_values() {
        assert_eq!(base64_vlq(0), "A");
        assert_eq!(base64_vlq(1), "C");
        assert_eq!(base64_vlq(-1), "D");
        assert_eq!(base64_vlq(16), "gB");
    }

    #[test]
    fn emits_release_and_debug_from_the_same_ir() {
        let source = SourceFile::new(
            FileId(0),
            "game.modl",
            "state score: Int = 0\nfn add(value: Int) -> Int:\n  return value + 1\non update:\n  score = add(score)\n",
        );
        let release = compile(&source, &AssetCatalog::default(), CompileMode::Release);
        let debug = compile(&source, &AssetCatalog::default(), CompileMode::Debug);
        assert!(release.analysis.diagnostics.is_empty());
        assert_eq!(release.analysis.ir, debug.analysis.ir);
        let release = release.generated.expect("release output");
        let debug = debug.generated.expect("debug output");
        assert!(!release.javascript.contains("yield {id:"));
        assert!(debug.javascript.contains("yield {id:"));
        assert!(debug.javascript.contains("locals:{s"));
        assert!(!release.relationships.is_empty());
        assert!(release.source_map_json.contains("\"version\":3"));
    }

    #[test]
    fn lowers_public_input_and_graphics_values_without_host_globals() {
        let source = SourceFile::new(
            FileId(0),
            "hardware.modl",
            "on update:\n  let pressed = btn(pad1, a)\n  if pressed:\n    pixel(1, 1, dither(1, 1, 2, 3, 8))\n",
        );
        let output = compile(&source, &AssetCatalog::default(), CompileMode::Release);
        assert!(output.analysis.diagnostics.is_empty());
        let javascript = &output.generated.expect("generated output").javascript;
        assert!(javascript.contains("consoleCall(\"btn\",[0,\"a\"]"));
        assert!(javascript.contains("consoleCall(\"dither\""));
    }

    #[test]
    fn lowers_waits_and_loops_to_explicit_task_branches() {
        let source = SourceFile::new(
            FileId(0),
            "task.modl",
            "task count():\n  var value = 0\n  while value < 2:\n    wait 1f\n    value += 1\n",
        );
        let output = compile(&source, &AssetCatalog::default(), CompileMode::Debug);
        let ir = output.analysis.ir.as_ref().expect("typed IR");
        let routine = ir.routines.first().expect("task routine");
        let program = TaskProgram::lower(routine);
        assert!(
            program
                .instructions
                .iter()
                .any(|instruction| matches!(instruction, TaskInstruction::Wait { .. }))
        );
        assert!(
            program
                .instructions
                .iter()
                .any(|instruction| matches!(instruction, TaskInstruction::Jump { .. }))
        );
        let javascript = output.generated.expect("generated output").javascript;
        assert!(javascript.contains("switch(task.pc)"));
        assert!(javascript.contains("task.wait="));
    }
}
