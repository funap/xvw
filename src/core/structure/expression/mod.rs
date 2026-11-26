#![allow(dead_code)]

mod ast;
mod encoding;
mod eval;
mod lexer;
mod parser;

#[cfg(test)]
mod tests;

pub use ast::ExprAST;
pub use encoding::decode_bytes;
#[cfg(test)]
pub use eval::G_EMPTY_BYTE_MAP;
pub use eval::{EvalContext, ExprEvaluator, ExprValue};
