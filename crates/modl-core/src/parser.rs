use serde::{Deserialize, Serialize};

use crate::{
    ast::{
        Assertion, AssignmentOperator, BinaryOperator, Block, Callback, CallbackKind,
        ConditionalBranch, Constant, Enum, EnumVariant, Expression, ExpressionKind, Function,
        IfStatement, Import, Item, Literal, MatchArm, MatchStatement, Module, Name, Parameter,
        Pattern, PatternKind, Record, RecordField, State, Statement, StatementKind, Task,
        TypeArgument, TypeKind, TypeNode, UnaryOperator, Visibility,
    },
    diagnostic::Diagnostic,
    lexer::{LexOutput, lex},
    span::{SourceFile, Span, Spanned},
    token::{Token, TokenKind},
};

/// Parser output remains available in the presence of recoverable diagnostics.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ParseOutput {
    pub module: Module,
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Runs the lexical and parsing phases for one source module.
#[must_use]
pub fn parse(source: &SourceFile) -> ParseOutput {
    let LexOutput {
        tokens,
        mut diagnostics,
    } = lex(source);
    let (module, parser_diagnostics) = Parser::new(&tokens, source.eof_span()).run();
    diagnostics.extend(parser_diagnostics);
    ParseOutput {
        module,
        tokens,
        diagnostics,
    }
}

struct Parser<'tokens> {
    tokens: &'tokens [Token],
    cursor: usize,
    eof: Span,
    diagnostics: Vec<Diagnostic>,
}

impl<'tokens> Parser<'tokens> {
    fn new(tokens: &'tokens [Token], eof: Span) -> Self {
        Self {
            tokens,
            cursor: 0,
            eof,
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> (Module, Vec<Diagnostic>) {
        let start = self.current().span;
        let mut items = Vec::new();
        self.skip_empty_lines();
        while !self.at(&TokenKind::Eof) {
            let before = self.cursor;
            if let Some(item) = self.parse_item() {
                items.push(item);
            } else {
                self.synchronize_line();
            }
            if self.cursor == before {
                self.advance();
            }
            self.skip_empty_lines();
        }
        let span = start.through(self.current().span);
        (Module { items, span }, self.diagnostics)
    }

    fn parse_item(&mut self) -> Option<Item> {
        let explicit_visibility = match self.current().kind {
            TokenKind::Pub => {
                self.advance();
                Some(Visibility::Public)
            }
            TokenKind::Private => {
                self.advance();
                Some(Visibility::Private)
            }
            _ => None,
        };
        let visibility = explicit_visibility.unwrap_or_default();
        if explicit_visibility.is_some()
            && !matches!(
                self.current().kind,
                TokenKind::Const
                    | TokenKind::State
                    | TokenKind::Fn
                    | TokenKind::Task
                    | TokenKind::Record
                    | TokenKind::Enum
            )
        {
            let token = self.current();
            self.diagnostics.push(
                Diagnostic::error(
                    "M012001",
                    token.span,
                    "visibility modifier does not apply to this module item",
                )
                .with_primary_label(
                    "visibility applies only to `const`, `state`, `fn`, `task`, `record`, or `enum`",
                ),
            );
            return None;
        }
        match &self.current().kind {
            TokenKind::Import => self.parse_import().map(Item::Import),
            TokenKind::Const => self.parse_constant(visibility).map(Item::Constant),
            TokenKind::State => self.parse_state(visibility).map(Item::State),
            TokenKind::Fn => self.parse_function(visibility).map(Item::Function),
            TokenKind::Task => self.parse_task(visibility).map(Item::Task),
            TokenKind::On => self.parse_callback().map(Item::Callback),
            TokenKind::Record => self.parse_record(visibility).map(Item::Record),
            TokenKind::Enum => self.parse_enum(visibility).map(Item::Enum),
            TokenKind::Assert => self.parse_assertion(true).map(Item::Assertion),
            _ => {
                let token = self.current();
                self.diagnostics.push(
                    Diagnostic::error(
                        "M012001",
                        token.span,
                        format!("{} cannot begin a module item", token.kind.description()),
                    )
                    .with_primary_label(
                        "expected `import`, `const`, `state`, `fn`, `task`, `on`, `record`, or `enum`",
                    ),
                );
                None
            }
        }
    }

    fn parse_import(&mut self) -> Option<Import> {
        let start = self.advance().span;
        let path = self.parse_path()?;
        let alias = if self.consume(&TokenKind::As).is_some() {
            self.expect_name("import alias")
        } else {
            None
        };
        let end = self.finish_line();
        Some(Import {
            path,
            alias,
            span: start.through(end),
        })
    }

    fn parse_constant(&mut self, visibility: Visibility) -> Option<Constant> {
        let start = self.advance().span;
        let name = self.expect_name("constant name")?;
        let type_annotation = if self.consume(&TokenKind::Colon).is_some() {
            self.parse_type()
        } else {
            None
        };
        self.expect(&TokenKind::Equal, "constant initializer");
        let value = self.parse_expression()?;
        let end = self.finish_line();
        Some(Constant {
            visibility,
            name,
            type_annotation,
            value,
            span: start.through(end),
        })
    }

    fn parse_state(&mut self, visibility: Visibility) -> Option<State> {
        let start = self.advance().span;
        let name = self.expect_name("state name")?;
        self.expect(&TokenKind::Colon, "explicit state type");
        let type_annotation = self.parse_type()?;
        self.expect(&TokenKind::Equal, "state initializer");
        let value = self.parse_expression()?;
        let end = self.finish_line();
        Some(State {
            visibility,
            name,
            type_annotation,
            value,
            span: start.through(end),
        })
    }

    fn parse_function(&mut self, visibility: Visibility) -> Option<Function> {
        let start = self.advance().span;
        let name = self.expect_name("function name")?;
        let parameters = self.parse_parameters()?;
        self.expect(&TokenKind::Arrow, "function return type");
        let return_type = self.parse_type()?;
        self.expect(&TokenKind::Colon, "function header");
        let (body, end) = self.parse_block();
        Some(Function {
            visibility,
            name,
            parameters,
            return_type,
            body,
            span: start.through(end),
        })
    }

    fn parse_task(&mut self, visibility: Visibility) -> Option<Task> {
        let start = self.advance().span;
        let name = self.expect_name("task name")?;
        let parameters = self.parse_parameters()?;
        self.expect(&TokenKind::Colon, "task header");
        let (body, end) = self.parse_block();
        Some(Task {
            visibility,
            name,
            parameters,
            body,
            span: start.through(end),
        })
    }

    fn parse_callback(&mut self) -> Option<Callback> {
        let start = self.advance().span;
        let kind = match self.current().kind {
            TokenKind::Start => CallbackKind::Start,
            TokenKind::Update => CallbackKind::Update,
            TokenKind::Draw => CallbackKind::Draw,
            TokenKind::Raster => CallbackKind::Raster,
            TokenKind::Identifier(ref name) if name == "menu" => CallbackKind::Menu,
            _ => {
                self.expected("system callback (`start`, `update`, `draw`, `raster`, or `menu`)");
                return None;
            }
        };
        self.advance();
        let parameters = if self.at(&TokenKind::LeftParen) {
            self.parse_parameters()?
        } else {
            Vec::new()
        };
        self.expect(&TokenKind::Colon, "callback header");
        let (body, end) = self.parse_block();
        Some(Callback {
            kind,
            parameters,
            body,
            span: start.through(end),
        })
    }

    fn parse_record(&mut self, visibility: Visibility) -> Option<Record> {
        let start = self.advance().span;
        let name = self.expect_name("record name")?;
        self.expect(&TokenKind::Colon, "record header");
        self.expect_line_break();
        if self.consume(&TokenKind::Indent).is_none() {
            self.expected_indented_block();
            return Some(Record {
                visibility,
                name,
                fields: Vec::new(),
                span: start.through(self.current().span),
            });
        }
        let mut fields = Vec::new();
        self.skip_empty_lines();
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            let field_start = self.current().span;
            let Some(field_name) = self.expect_name("record field") else {
                self.synchronize_line();
                continue;
            };
            self.expect(&TokenKind::Colon, "record field type");
            let Some(type_annotation) = self.parse_type() else {
                self.synchronize_line();
                continue;
            };
            let default = if self.consume(&TokenKind::Equal).is_some() {
                self.parse_expression()
            } else {
                None
            };
            let end = self.finish_line();
            fields.push(RecordField {
                name: field_name,
                type_annotation,
                default,
                span: field_start.through(end),
            });
            self.skip_empty_lines();
        }
        let end = self
            .consume(&TokenKind::Dedent)
            .map_or(self.eof, |token| token.span);
        Some(Record {
            visibility,
            name,
            fields,
            span: start.through(end),
        })
    }

    fn parse_enum(&mut self, visibility: Visibility) -> Option<Enum> {
        let start = self.advance().span;
        let name = self.expect_name("enum name")?;
        self.expect(&TokenKind::Colon, "enum header");
        self.expect_line_break();
        if self.consume(&TokenKind::Indent).is_none() {
            self.expected_indented_block();
            return Some(Enum {
                visibility,
                name,
                variants: Vec::new(),
                span: start.through(self.current().span),
            });
        }
        let mut variants = Vec::new();
        self.skip_empty_lines();
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            let variant_start = self.current().span;
            let Some(variant_name) = self.expect_name("enum variant") else {
                self.synchronize_line();
                continue;
            };
            let mut fields = Vec::new();
            if self.consume(&TokenKind::LeftParen).is_some() {
                if !self.at(&TokenKind::RightParen) {
                    loop {
                        if let Some(field) = self.parse_type() {
                            fields.push(field);
                        }
                        if self.consume(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                self.expect(&TokenKind::RightParen, "enum variant fields");
            }
            let end = self.finish_line();
            variants.push(EnumVariant {
                name: variant_name,
                fields,
                span: variant_start.through(end),
            });
            self.skip_empty_lines();
        }
        let end = self
            .consume(&TokenKind::Dedent)
            .map_or(self.eof, |token| token.span);
        Some(Enum {
            visibility,
            name,
            variants,
            span: start.through(end),
        })
    }

    fn parse_parameters(&mut self) -> Option<Vec<Parameter>> {
        self.expect(&TokenKind::LeftParen, "parameter list");
        let mut parameters = Vec::new();
        if !self.at(&TokenKind::RightParen) {
            loop {
                let start = self.current().span;
                let name = self.expect_name("parameter name")?;
                self.expect(&TokenKind::Colon, "parameter type");
                let type_annotation = self.parse_type()?;
                parameters.push(Parameter {
                    span: start.through(type_annotation.span),
                    name,
                    type_annotation,
                });
                if self.consume(&TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RightParen, "parameter list");
        Some(parameters)
    }

    fn parse_type(&mut self) -> Option<TypeNode> {
        if let Some(left) = self.consume(&TokenKind::LeftBracket) {
            let element = self.parse_type()?;
            self.expect(&TokenKind::Comma, "fixed array capacity");
            let capacity = self.expect_capacity()?;
            let right = self.expect(&TokenKind::RightBracket, "fixed array type");
            return Some(TypeNode {
                span: left.span.through(right.span),
                kind: TypeKind::Array {
                    element: Box::new(element),
                    length: capacity.value,
                },
            });
        }

        let path = self.parse_path().or_else(|| {
            self.expected("type name");
            None
        })?;
        let start = path[0].span;
        if self.consume(&TokenKind::LeftBracket).is_none() {
            let end = path.last().map_or(start, |part| part.span);
            return Some(TypeNode {
                kind: TypeKind::Named(path),
                span: start.through(end),
            });
        }

        let mut arguments = Vec::new();
        if !self.at(&TokenKind::RightBracket) {
            loop {
                if matches!(self.current().kind, TokenKind::Int(_)) {
                    arguments.push(TypeArgument::Capacity(self.expect_capacity()?));
                } else {
                    arguments.push(TypeArgument::Type(self.parse_type()?));
                }
                if self.consume(&TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        let right = self.expect(&TokenKind::RightBracket, "generic type arguments");
        Some(TypeNode {
            kind: TypeKind::Generic {
                name: path,
                arguments,
            },
            span: start.through(right.span),
        })
    }

    fn expect_capacity(&mut self) -> Option<Spanned<u32>> {
        let token = self.current().clone();
        let TokenKind::Int(value) = token.kind else {
            self.expected("non-negative fixed capacity");
            return None;
        };
        self.advance();
        if let Ok(value) = u32::try_from(value) {
            Some(Spanned::new(value, token.span))
        } else {
            self.diagnostics.push(
                Diagnostic::error(
                    "M012006",
                    token.span,
                    "fixed capacity must fit a non-negative 32-bit integer",
                )
                .with_primary_label("invalid capacity"),
            );
            None
        }
    }

    fn parse_block(&mut self) -> (Block, Span) {
        let line_end = self.expect_line_break();
        if self.consume(&TokenKind::Indent).is_none() {
            self.expected_indented_block();
            return (Vec::new(), line_end);
        }
        let mut statements = Vec::new();
        self.skip_empty_lines();
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            let before = self.cursor;
            if let Some(statement) = self.parse_statement() {
                statements.push(statement);
            } else {
                self.synchronize_line();
            }
            if self.cursor == before {
                self.advance();
            }
            self.skip_empty_lines();
        }
        let end = self
            .consume(&TokenKind::Dedent)
            .map_or(self.eof, |token| token.span);
        (statements, end)
    }

    fn parse_statement(&mut self) -> Option<Statement> {
        match self.current().kind {
            TokenKind::Let => self.parse_binding(false),
            TokenKind::Var => self.parse_binding(true),
            TokenKind::Return => Some(self.parse_return()),
            TokenKind::Break => Some(self.parse_unit_statement(StatementKind::Break)),
            TokenKind::Continue => Some(self.parse_unit_statement(StatementKind::Continue)),
            TokenKind::Wait => self.parse_prefixed_expression(StatementKind::Wait),
            TokenKind::Start => self.parse_prefixed_expression(StatementKind::Start),
            TokenKind::Assert => self.parse_assertion(false).map(|assertion| Statement {
                span: assertion.span,
                kind: StatementKind::Assert(assertion),
            }),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Match => self.parse_match(),
            _ => self.parse_expression_statement(),
        }
    }

    fn parse_binding(&mut self, mutable: bool) -> Option<Statement> {
        let start = self.advance().span;
        let name = self.expect_name("local name")?;
        self.expect(&TokenKind::Equal, "local initializer");
        let value = self.parse_expression()?;
        let end = self.finish_line();
        Some(Statement {
            span: start.through(end),
            kind: StatementKind::Let {
                mutable,
                name,
                value,
            },
        })
    }

    fn parse_return(&mut self) -> Statement {
        let start = self.advance().span;
        let value = if self.at_line_end() {
            None
        } else {
            self.parse_expression()
        };
        let end = self.finish_line();
        Statement {
            span: start.through(end),
            kind: StatementKind::Return(value),
        }
    }

    fn parse_unit_statement(&mut self, kind: StatementKind) -> Statement {
        let start = self.advance().span;
        let end = self.finish_line();
        Statement {
            kind,
            span: start.through(end),
        }
    }

    fn parse_prefixed_expression(
        &mut self,
        constructor: fn(Expression) -> StatementKind,
    ) -> Option<Statement> {
        let start = self.advance().span;
        let expression = self.parse_expression()?;
        let end = self.finish_line();
        Some(Statement {
            kind: constructor(expression),
            span: start.through(end),
        })
    }

    fn parse_assertion(&mut self, module_level: bool) -> Option<Assertion> {
        let start = self.advance().span;
        let condition = self.parse_expression()?;
        let message = if self.consume(&TokenKind::Comma).is_some() {
            self.parse_expression()
        } else {
            None
        };
        let end = self.finish_line();
        let assertion = Assertion {
            condition,
            message,
            span: start.through(end),
        };
        if module_level && !matches!(assertion.condition.kind, ExpressionKind::Literal(_)) {
            // Constness is decided later; retaining this marker here helps the explorer explain the phase split.
        }
        Some(assertion)
    }

    fn parse_if(&mut self) -> Option<Statement> {
        let start = self.advance().span;
        let condition = self.parse_expression()?;
        self.expect(&TokenKind::Colon, "if condition");
        let (body, mut end) = self.parse_block();
        let mut branches = vec![ConditionalBranch {
            condition,
            body,
            span: start.through(end),
        }];
        while let Some(elif) = self.consume(&TokenKind::Elif) {
            let condition = self.parse_expression()?;
            self.expect(&TokenKind::Colon, "elif condition");
            let (body, branch_end) = self.parse_block();
            branches.push(ConditionalBranch {
                condition,
                body,
                span: elif.span.through(branch_end),
            });
            end = branch_end;
        }
        let else_body = if self.consume(&TokenKind::Else).is_some() {
            self.expect(&TokenKind::Colon, "else branch");
            let (body, branch_end) = self.parse_block();
            end = branch_end;
            Some(body)
        } else {
            None
        };
        Some(Statement {
            span: start.through(end),
            kind: StatementKind::If(IfStatement {
                branches,
                else_body,
            }),
        })
    }

    fn parse_while(&mut self) -> Option<Statement> {
        let start = self.advance().span;
        let condition = self.parse_expression()?;
        self.expect(&TokenKind::Colon, "while condition");
        let (body, end) = self.parse_block();
        Some(Statement {
            span: start.through(end),
            kind: StatementKind::While { condition, body },
        })
    }

    fn parse_for(&mut self) -> Option<Statement> {
        let start = self.advance().span;
        let binding = self.expect_name("loop binding")?;
        self.expect(&TokenKind::In, "for loop");
        let iterable = self.parse_expression()?;
        self.expect(&TokenKind::Colon, "for loop header");
        let (body, end) = self.parse_block();
        Some(Statement {
            span: start.through(end),
            kind: StatementKind::For {
                binding,
                iterable,
                body,
            },
        })
    }

    fn parse_match(&mut self) -> Option<Statement> {
        let start = self.advance().span;
        let subject = self.parse_expression()?;
        self.expect(&TokenKind::Colon, "match subject");
        self.expect_line_break();
        if self.consume(&TokenKind::Indent).is_none() {
            self.expected_indented_block();
            return None;
        }
        let mut arms = Vec::new();
        self.skip_empty_lines();
        while !self.at(&TokenKind::Dedent) && !self.at(&TokenKind::Eof) {
            let arm_start = self.current().span;
            let pattern = if self.consume(&TokenKind::Case).is_some() {
                self.parse_pattern()?
            } else if let Some(otherwise) = self.consume(&TokenKind::Else) {
                Pattern {
                    kind: PatternKind::Wildcard,
                    span: otherwise.span,
                }
            } else {
                self.diagnostics.push(
                    Diagnostic::error(
                        "M012008",
                        self.current().span,
                        "match arms must begin with `case` or `else`",
                    )
                    .with_primary_label("invalid match arm"),
                );
                self.synchronize_line();
                continue;
            };
            self.expect(&TokenKind::Colon, "match arm");
            let (body, arm_end) = self.parse_block();
            arms.push(MatchArm {
                pattern,
                body,
                span: arm_start.through(arm_end),
            });
            self.skip_empty_lines();
        }
        let end = self
            .consume(&TokenKind::Dedent)
            .map_or(self.eof, |token| token.span);
        Some(Statement {
            span: start.through(end),
            kind: StatementKind::Match(MatchStatement { subject, arms }),
        })
    }

    fn parse_pattern(&mut self) -> Option<Pattern> {
        let token = self.current().clone();
        let literal = match token.kind {
            TokenKind::Int(value) => Some(Literal::Int(value)),
            TokenKind::Num(value) => Some(Literal::Num(value)),
            TokenKind::Text(value) => Some(Literal::Text(value)),
            TokenKind::True => Some(Literal::Bool(true)),
            TokenKind::False => Some(Literal::Bool(false)),
            TokenKind::None => Some(Literal::None),
            _ => None,
        };
        if let Some(literal) = literal {
            self.advance();
            return Some(Pattern {
                kind: PatternKind::Literal(literal),
                span: token.span,
            });
        }

        let path = self.parse_path()?;
        let start = path[0].span;
        if path.len() == 1 && path[0].value == "_" {
            return Some(Pattern {
                kind: PatternKind::Wildcard,
                span: start,
            });
        }
        let mut bindings = Vec::new();
        let has_arguments = self.consume(&TokenKind::LeftParen).is_some();
        if has_arguments {
            if !self.at(&TokenKind::RightParen) {
                loop {
                    bindings.push(self.expect_name("pattern binding")?);
                    if self.consume(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
            }
            self.expect(&TokenKind::RightParen, "variant pattern");
        }
        let end = self.previous().span;
        let is_variant = has_arguments
            || path.len() > 1
            || path[0]
                .value
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_uppercase);
        Some(Pattern {
            span: start.through(end),
            kind: if is_variant {
                PatternKind::Variant { path, bindings }
            } else {
                PatternKind::Binding(path.into_iter().next().expect("path is non-empty"))
            },
        })
    }

    fn parse_expression_statement(&mut self) -> Option<Statement> {
        let expression = self.parse_expression()?;
        let start = expression.span;
        let operator = match self.current().kind {
            TokenKind::Equal => Some(AssignmentOperator::Assign),
            TokenKind::PlusEqual => Some(AssignmentOperator::Add),
            TokenKind::MinusEqual => Some(AssignmentOperator::Subtract),
            TokenKind::StarEqual => Some(AssignmentOperator::Multiply),
            TokenKind::SlashEqual => Some(AssignmentOperator::Divide),
            _ => None,
        };
        let kind = if let Some(operator) = operator {
            self.advance();
            let value = self.parse_expression()?;
            StatementKind::Assignment {
                target: expression,
                operator,
                value,
            }
        } else {
            StatementKind::Expression(expression)
        };
        let end = self.finish_line();
        Some(Statement {
            kind,
            span: start.through(end),
        })
    }

    fn parse_expression(&mut self) -> Option<Expression> {
        self.parse_binary(1)
    }

    fn parse_binary(&mut self, minimum_precedence: u8) -> Option<Expression> {
        let mut left = self.parse_unary()?;
        while let Some((operator, precedence)) = binary_operator(&self.current().kind) {
            if precedence < minimum_precedence {
                break;
            }
            self.advance();
            let right = self.parse_binary(precedence + 1)?;
            let span = left.span.through(right.span);
            left = Expression {
                span,
                kind: ExpressionKind::Binary {
                    left: Box::new(left),
                    operator,
                    right: Box::new(right),
                },
            };
        }
        Some(left)
    }

    fn parse_unary(&mut self) -> Option<Expression> {
        let operator = match self.current().kind {
            TokenKind::Minus => Some(UnaryOperator::Negate),
            TokenKind::Plus => Some(UnaryOperator::Positive),
            TokenKind::Not => Some(UnaryOperator::Not),
            TokenKind::Tilde => Some(UnaryOperator::BitNot),
            _ => None,
        };
        if let Some(operator) = operator {
            let start = self.advance().span;
            let operand = self.parse_unary()?;
            return Some(Expression {
                span: start.through(operand.span),
                kind: ExpressionKind::Unary {
                    operator,
                    operand: Box::new(operand),
                },
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Option<Expression> {
        let mut expression = self.parse_primary()?;
        loop {
            if self.consume(&TokenKind::LeftParen).is_some() {
                let mut arguments = Vec::new();
                if !self.at(&TokenKind::RightParen) {
                    loop {
                        arguments.push(self.parse_expression()?);
                        if self.consume(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                let right = self.expect(&TokenKind::RightParen, "call arguments");
                expression = Expression {
                    span: expression.span.through(right.span),
                    kind: ExpressionKind::Call {
                        callee: Box::new(expression),
                        arguments,
                    },
                };
            } else if self.consume(&TokenKind::LeftBracket).is_some() {
                let index = self.parse_expression()?;
                let right = self.expect(&TokenKind::RightBracket, "index expression");
                expression = Expression {
                    span: expression.span.through(right.span),
                    kind: ExpressionKind::Index {
                        subject: Box::new(expression),
                        index: Box::new(index),
                    },
                };
            } else if self.consume(&TokenKind::Dot).is_some() {
                let field = self.expect_name("field name")?;
                expression = Expression {
                    span: expression.span.through(field.span),
                    kind: ExpressionKind::Field {
                        subject: Box::new(expression),
                        field,
                    },
                };
            } else {
                break;
            }
        }
        Some(expression)
    }

    fn parse_primary(&mut self) -> Option<Expression> {
        let token = self.current().clone();
        let literal = match &token.kind {
            TokenKind::Int(value) => Some(Literal::Int(*value)),
            TokenKind::Num(value) => Some(Literal::Num(*value)),
            TokenKind::Text(value) => Some(Literal::Text(value.clone())),
            TokenKind::Frames(value) => Some(Literal::Frames(*value)),
            TokenKind::Seconds(value) => Some(Literal::Seconds(*value)),
            TokenKind::True => Some(Literal::Bool(true)),
            TokenKind::False => Some(Literal::Bool(false)),
            TokenKind::None => Some(Literal::None),
            _ => None,
        };
        if let Some(literal) = literal {
            self.advance();
            return Some(Expression {
                kind: ExpressionKind::Literal(literal),
                span: token.span,
            });
        }
        match token.kind {
            TokenKind::Identifier(name) => {
                self.advance();
                Some(Expression {
                    kind: ExpressionKind::Name(vec![Spanned::new(name, token.span)]),
                    span: token.span,
                })
            }
            TokenKind::Asset(name) => {
                self.advance();
                Some(Expression {
                    kind: ExpressionKind::Asset(Spanned::new(name, token.span)),
                    span: token.span,
                })
            }
            TokenKind::LeftParen => {
                let start = self.advance().span;
                let mut expression = self.parse_expression()?;
                let right = self.expect(&TokenKind::RightParen, "grouped expression");
                expression.span = start.through(right.span);
                Some(expression)
            }
            TokenKind::LeftBracket => {
                let start = self.advance().span;
                let mut elements = Vec::new();
                if !self.at(&TokenKind::RightBracket) {
                    loop {
                        elements.push(self.parse_expression()?);
                        if self.consume(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                let right = self.expect(&TokenKind::RightBracket, "array literal");
                Some(Expression {
                    kind: ExpressionKind::Array(elements),
                    span: start.through(right.span),
                })
            }
            _ => {
                self.diagnostics.push(
                    Diagnostic::error("M012003", token.span, "expected an expression")
                        .with_primary_label(format!("found {}", token.kind.description())),
                );
                None
            }
        }
    }

    fn parse_path(&mut self) -> Option<Vec<Name>> {
        let mut path = vec![self.expect_name("name")?];
        while self.at(&TokenKind::Dot)
            && self
                .tokens
                .get(self.cursor + 1)
                .is_some_and(|token| matches!(token.kind, TokenKind::Identifier(_)))
        {
            self.advance();
            path.push(self.expect_name("path segment")?);
        }
        Some(path)
    }

    fn expect_name(&mut self, context: &'static str) -> Option<Name> {
        let token = self.current().clone();
        if let TokenKind::Identifier(name) = token.kind {
            self.advance();
            Some(Spanned::new(name, token.span))
        } else {
            self.diagnostics.push(
                Diagnostic::error(
                    "M012002",
                    token.span,
                    format!("expected identifier for {context}"),
                )
                .with_primary_label(format!("found {}", token.kind.description())),
            );
            None
        }
    }

    fn expect(&mut self, kind: &TokenKind, context: &'static str) -> Token {
        if self.at(kind) {
            return self.advance().clone();
        }
        let token = self.current().clone();
        self.diagnostics.push(
            Diagnostic::error(
                "M012002",
                token.span,
                format!("expected {} in {context}", kind.description()),
            )
            .with_primary_label(format!("found {}", token.kind.description())),
        );
        Token::new(
            kind.clone(),
            Span::new(token.span.file, token.span.start, token.span.start),
        )
    }

    fn expected(&mut self, expected: &'static str) {
        let token = self.current();
        self.diagnostics.push(
            Diagnostic::error("M012002", token.span, format!("expected {expected}"))
                .with_primary_label(format!("found {}", token.kind.description())),
        );
    }

    fn expected_indented_block(&mut self) {
        self.diagnostics.push(
            Diagnostic::error("M012004", self.current().span, "expected an indented block")
                .with_primary_label("indent at least one statement beneath this header"),
        );
    }

    fn expect_line_break(&mut self) -> Span {
        if self.at_comment() {
            self.advance();
        }
        self.expect(&TokenKind::Newline, "end of line").span
    }

    fn finish_line(&mut self) -> Span {
        if self.at_comment() {
            self.advance();
        }
        if self.at(&TokenKind::Newline) {
            return self.advance().span;
        }
        let start = self.current().span;
        self.diagnostics.push(
            Diagnostic::error("M012002", start, "expected the statement to end here")
                .with_primary_label(format!("found {}", self.current().kind.description())),
        );
        self.synchronize_line()
    }

    fn synchronize_line(&mut self) -> Span {
        let mut end = self.current().span;
        while !self.at(&TokenKind::Newline)
            && !self.at(&TokenKind::Dedent)
            && !self.at(&TokenKind::Eof)
        {
            end = self.advance().span;
        }
        if self.at(&TokenKind::Newline) {
            end = self.advance().span;
        }
        end
    }

    fn skip_empty_lines(&mut self) {
        loop {
            if self.at_comment() {
                self.advance();
                if self.at(&TokenKind::Newline) {
                    self.advance();
                }
            } else if self.at(&TokenKind::Newline) {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn at_line_end(&self) -> bool {
        self.at_comment()
            || self.at(&TokenKind::Newline)
            || self.at(&TokenKind::Dedent)
            || self.at(&TokenKind::Eof)
    }

    fn consume(&mut self, kind: &TokenKind) -> Option<Token> {
        self.at(kind).then(|| self.advance().clone())
    }

    fn at_comment(&self) -> bool {
        matches!(self.current().kind, TokenKind::Comment(_))
    }

    fn at(&self, kind: &TokenKind) -> bool {
        self.current().kind.same_variant(kind)
    }

    fn current(&self) -> &Token {
        self.tokens
            .get(self.cursor)
            .or_else(|| self.tokens.last())
            .expect("lexer always emits EOF")
    }

    fn previous(&self) -> &Token {
        self.tokens
            .get(self.cursor.saturating_sub(1))
            .expect("parser only asks for a previous token after advancing")
    }

    fn advance(&mut self) -> &Token {
        let index = self.cursor;
        if !self.at(&TokenKind::Eof) {
            self.cursor += 1;
        }
        &self.tokens[index]
    }
}

fn binary_operator(kind: &TokenKind) -> Option<(BinaryOperator, u8)> {
    Some(match kind {
        TokenKind::Or => (BinaryOperator::Or, 1),
        TokenKind::And => (BinaryOperator::And, 2),
        TokenKind::EqualEqual => (BinaryOperator::Equal, 3),
        TokenKind::BangEqual => (BinaryOperator::NotEqual, 3),
        TokenKind::Less => (BinaryOperator::Less, 4),
        TokenKind::LessEqual => (BinaryOperator::LessEqual, 4),
        TokenKind::Greater => (BinaryOperator::Greater, 4),
        TokenKind::GreaterEqual => (BinaryOperator::GreaterEqual, 4),
        TokenKind::Range => (BinaryOperator::Range, 5),
        TokenKind::Pipe => (BinaryOperator::BitOr, 6),
        TokenKind::Caret => (BinaryOperator::BitXor, 7),
        TokenKind::Ampersand => (BinaryOperator::BitAnd, 8),
        TokenKind::ShiftLeft => (BinaryOperator::ShiftLeft, 9),
        TokenKind::ShiftRight => (BinaryOperator::ShiftRight, 9),
        TokenKind::ShiftRightUnsigned => (BinaryOperator::ShiftRightUnsigned, 9),
        TokenKind::Plus => (BinaryOperator::Add, 10),
        TokenKind::Minus => (BinaryOperator::Subtract, 10),
        TokenKind::Star => (BinaryOperator::Multiply, 11),
        TokenKind::Slash => (BinaryOperator::Divide, 11),
        TokenKind::Percent => (BinaryOperator::Remainder, 11),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        FileId, SourceFile,
        ast::{Item, Visibility},
        parse,
    };

    #[test]
    fn parses_explicit_public_and_private_declarations() {
        let source = SourceFile::new(
            FileId(0),
            "visibility.modl",
            "pub fn shown() -> Int:\n  return 1\nprivate fn hidden() -> Int:\n  return 2\n",
        );
        let output = parse(&source);
        assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
        assert!(matches!(
            &output.module.items[0],
            Item::Function(function) if function.visibility == Visibility::Public
        ));
        assert!(matches!(
            &output.module.items[1],
            Item::Function(function) if function.visibility == Visibility::Private
        ));
    }

    #[test]
    fn parses_required_declaration_shapes() {
        let source = SourceFile::new(
            FileId(0),
            "game.modl",
            r"state score: Int = 0
record Player:
  pos: Vec2
enum Mode:
  Play
  Win(Int)
fn move(pos: Vec2, vel: Vec2) -> Vec2:
  return pos + vel
task flash():
  wait 2f
on update:
  start flash()
",
        );
        let output = parse(&source);
        assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
        assert_eq!(output.module.items.len(), 6);
        assert!(matches!(output.module.items[0], Item::State(_)));
        assert!(matches!(output.module.items[5], Item::Callback(_)));
    }

    #[test]
    fn expression_precedence_is_not_flat() {
        let source = SourceFile::new(FileId(0), "math.modl", "const result = 1 + 2 * 3 == 7\n");
        let output = parse(&source);
        assert!(output.diagnostics.is_empty(), "{:#?}", output.diagnostics);
        let json = serde_json::to_string(&output.module).expect("AST serializes");
        let add = json.find("Add").expect("contains addition");
        let multiply = json.find("Multiply").expect("contains multiplication");
        assert!(add < multiply);
    }

    #[test]
    fn recovers_after_a_malformed_item() {
        let source = SourceFile::new(
            FileId(0),
            "bad.modl",
            "state missing Int = 0\nstate valid: Int = 1\n",
        );
        let output = parse(&source);
        assert!(output.diagnostics.iter().any(|item| item.code == "M012002"));
        assert_eq!(output.module.items.len(), 2);
        assert!(matches!(output.module.items[1], Item::State(_)));
    }
}
