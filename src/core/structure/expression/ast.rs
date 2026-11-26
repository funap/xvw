use super::parser::ASTParser;

/// Abstract Syntax Tree (AST) for pre-compiled Kaitai Struct expressions
#[derive(Debug, Clone, PartialEq)]
pub enum ExprAST {
    Number(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Array(Vec<ExprAST>),
    Identifier(String),
    MemberAccess {
        base: Box<ExprAST>,
        member: String,
        is_enum: bool,
    },
    IndexAccess {
        base: Box<ExprAST>,
        index: Box<ExprAST>,
    },
    MethodCall {
        base: Box<ExprAST>,
        method: String,
        args: Vec<ExprAST>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<ExprAST>,
    },
    Binary {
        op: BinaryOp,
        left: Box<ExprAST>,
        right: Box<ExprAST>,
    },
    Ternary {
        cond: Box<ExprAST>,
        then_branch: Box<ExprAST>,
        else_branch: Box<ExprAST>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Shl,
    Shr,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    BitAnd,
    BitOr,
    BitXor,
    LogicalAnd,
    LogicalOr,
}

impl ExprAST {
    pub fn compile(expr: &str) -> Option<ExprAST> {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            return None;
        }
        let mut parser = ASTParser::new(trimmed);
        Some(parser.parse_expression())
    }
}
