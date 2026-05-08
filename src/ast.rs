#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Literal(Literal),
    Identifier(String),
    UnaryOp {
        operator: UnaryOperator,
        expr: Box<Expression>,
    },
    BinaryOp {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
    },
    MemberAccess {
        object: Box<Expression>,
        member: String,
    },
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },
    New {
        class_name: String,
        arguments: Vec<Expression>,
    },
    Array(Vec<Expression>),
    Index {
        collection: Box<Expression>,
        index: Box<Expression>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOperator {
    Negate,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equal,
    NotEqual,
    Less,
    Greater,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignmentTarget {
    Identifier(String),
    MemberAccess {
        object: Expression,
        member: String,
    },
    Index {
        collection: Expression,
        index: Expression,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub type_name: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Parameter>,
    pub return_type: Option<String>,
    pub body: Box<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassField {
    pub is_mutable: bool,
    pub field_type: String,
    pub name: String,
    pub default_value: Option<Expression>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    VariableDecl {
        is_mutable: bool,
        var_type: String,
        name: String,
        value: Expression,
    },
    LetDecl {
        is_mutable: bool,
        name: String,
        value: Expression,
    },
    Assignment {
        target: AssignmentTarget,
        value: Expression,
    },
    Print(Expression),
    Expression(Expression),
    Block(Vec<Statement>),

    If {
        condition: Expression,
        then_branch: Box<Statement>,
        else_branch: Option<Box<Statement>>,
    },

    WhileLoop {
        condition: Expression,
        body: Box<Statement>,
    },

    ForLoop {
        init: Box<Statement>,
        condition: Expression,
        increment: Box<Statement>,
        body: Box<Statement>,
    },
    ForIn {
        var: String,
        iterable: Expression,
        body: Box<Statement>,
    },

    Break,

    FunctionDecl(FunctionDecl),
    Return(Option<Expression>),

    ClassDecl {
        name: String,
        fields: Vec<ClassField>,
        methods: Vec<FunctionDecl>,
    },

    ModuleDecl {
        name: String,
        body: Vec<Statement>,
    },

    Import(String),
}