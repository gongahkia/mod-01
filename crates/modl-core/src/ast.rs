use serde::{Deserialize, Serialize};

use crate::span::{Span, Spanned};

pub type Name = Spanned<String>;

/// Project-link visibility. Declarations are public unless explicitly marked private.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    #[default]
    Public,
    Private,
}

/// Parsed MODL module. Name resolution intentionally happens in a later phase.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Module {
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum Item {
    Import(Import),
    Constant(Constant),
    State(State),
    Function(Function),
    Task(Task),
    Callback(Callback),
    Record(Record),
    Enum(Enum),
    Assertion(Assertion),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Import {
    pub path: Vec<Name>,
    pub alias: Option<Name>,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Constant {
    pub visibility: Visibility,
    pub name: Name,
    pub type_annotation: Option<TypeNode>,
    pub value: Expression,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct State {
    pub visibility: Visibility,
    pub name: Name,
    pub type_annotation: TypeNode,
    pub value: Expression,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Parameter {
    pub name: Name,
    pub type_annotation: TypeNode,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Function {
    pub visibility: Visibility,
    pub name: Name,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeNode,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Task {
    pub visibility: Visibility,
    pub name: Name,
    pub parameters: Vec<Parameter>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CallbackKind {
    Start,
    Update,
    Draw,
    Raster,
    Menu,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Callback {
    pub kind: CallbackKind,
    pub parameters: Vec<Parameter>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Record {
    pub visibility: Visibility,
    pub name: Name,
    pub fields: Vec<RecordField>,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RecordField {
    pub name: Name,
    pub type_annotation: TypeNode,
    pub default: Option<Expression>,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Enum {
    pub visibility: Visibility,
    pub name: Name,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EnumVariant {
    pub name: Name,
    pub fields: Vec<TypeNode>,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Assertion {
    pub condition: Expression,
    pub message: Option<Expression>,
    pub span: Span,
}

pub type Block = Vec<Statement>;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum StatementKind {
    Let {
        mutable: bool,
        name: Name,
        value: Expression,
    },
    Assignment {
        target: Expression,
        operator: AssignmentOperator,
        value: Expression,
    },
    Expression(Expression),
    Return(Option<Expression>),
    Break,
    Continue,
    Wait(Expression),
    Start(Expression),
    Assert(Assertion),
    If(IfStatement),
    While {
        condition: Expression,
        body: Block,
    },
    For {
        binding: Name,
        iterable: Expression,
        body: Block,
    },
    Match(MatchStatement),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AssignmentOperator {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct IfStatement {
    pub branches: Vec<ConditionalBranch>,
    pub else_body: Option<Block>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ConditionalBranch {
    pub condition: Expression,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MatchStatement {
    pub subject: Expression,
    pub arms: Vec<MatchArm>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum PatternKind {
    Wildcard,
    Binding(Name),
    Variant {
        path: Vec<Name>,
        bindings: Vec<Name>,
    },
    Literal(Literal),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TypeNode {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum TypeKind {
    Named(Vec<Name>),
    Generic {
        name: Vec<Name>,
        arguments: Vec<TypeArgument>,
    },
    Array {
        element: Box<TypeNode>,
        length: u32,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum TypeArgument {
    Type(TypeNode),
    Capacity(Spanned<u32>),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Span,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum ExpressionKind {
    Literal(Literal),
    Name(Vec<Name>),
    Asset(Name),
    Array(Vec<Expression>),
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
    },
    Binary {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
    },
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },
    Field {
        subject: Box<Expression>,
        field: Name,
    },
    Index {
        subject: Box<Expression>,
        index: Box<Expression>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value")]
pub enum Literal {
    Int(i64),
    Num(f64),
    Bool(bool),
    Text(String),
    None,
    Frames(u32),
    Seconds(f64),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum UnaryOperator {
    Negate,
    Positive,
    Not,
    BitNot,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BinaryOperator {
    Or,
    And,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Range,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    ShiftRightUnsigned,
}

impl BinaryOperator {
    #[must_use]
    pub const fn bitwise_call(self) -> Option<&'static str> {
        match self {
            Self::BitAnd => Some("bit_and"),
            Self::BitOr => Some("bit_or"),
            Self::BitXor => Some("bit_xor"),
            Self::ShiftLeft => Some("bit_shl"),
            Self::ShiftRight => Some("bit_shr"),
            Self::ShiftRightUnsigned => Some("bit_ushr"),
            _ => None,
        }
    }
}
