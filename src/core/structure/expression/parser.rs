use super::ast::{BinaryOp, ExprAST, UnaryOp};
use super::lexer::{Lexer, Token};

pub struct ASTParser<'a> {
    lexer: Lexer<'a>,
    current_token: Token,
}

impl<'a> ASTParser<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut lexer = Lexer::new(input);
        let current_token = lexer.next_token();
        Self { lexer, current_token }
    }

    fn advance(&mut self) {
        self.current_token = self.lexer.next_token();
    }

    fn take_token(&mut self) -> Token {
        let tok = std::mem::replace(&mut self.current_token, Token::EOF);
        self.advance();
        tok
    }

    pub fn parse_expression(&mut self) -> ExprAST {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> ExprAST {
        let cond = self.parse_or();
        if self.current_token == Token::Question {
            self.advance();
            let then_branch = self.parse_ternary();
            if self.current_token == Token::Colon {
                self.advance();
            }
            let else_branch = self.parse_ternary();
            ExprAST::Ternary {
                cond: Box::new(cond),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
            }
        } else {
            cond
        }
    }

    fn parse_or(&mut self) -> ExprAST {
        let mut left = self.parse_and();
        while matches!(self.current_token, Token::Or) || matches!(self.current_token, Token::Identifier(ref s) if s == "or") {
            self.advance();
            let right = self.parse_and();
            left = ExprAST::Binary {
                op: BinaryOp::LogicalOr,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_and(&mut self) -> ExprAST {
        let mut left = self.parse_comparison();
        while matches!(self.current_token, Token::And) || matches!(self.current_token, Token::Identifier(ref s) if s == "and") {
            self.advance();
            let right = self.parse_comparison();
            left = ExprAST::Binary {
                op: BinaryOp::LogicalAnd,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_comparison(&mut self) -> ExprAST {
        let mut left = self.parse_bit_or();
        while matches!(
            self.current_token,
            Token::Equal | Token::NotEqual | Token::Greater | Token::GreaterEqual | Token::Less | Token::LessEqual
        ) {
            let tok = self.take_token();
            let op = match tok {
                Token::Equal => BinaryOp::Equal,
                Token::NotEqual => BinaryOp::NotEqual,
                Token::Greater => BinaryOp::Greater,
                Token::GreaterEqual => BinaryOp::GreaterEqual,
                Token::Less => BinaryOp::Less,
                Token::LessEqual => BinaryOp::LessEqual,
                _ => unreachable!(),
            };
            let right = self.parse_bit_or();
            left = ExprAST::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_bit_or(&mut self) -> ExprAST {
        let mut left = self.parse_bit_xor();
        while matches!(self.current_token, Token::Pipe) {
            self.advance();
            let right = self.parse_bit_xor();
            left = ExprAST::Binary {
                op: BinaryOp::BitOr,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_bit_xor(&mut self) -> ExprAST {
        let mut left = self.parse_bit_and();
        while matches!(self.current_token, Token::Caret) {
            self.advance();
            let right = self.parse_bit_and();
            left = ExprAST::Binary {
                op: BinaryOp::BitXor,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_bit_and(&mut self) -> ExprAST {
        let mut left = self.parse_shift();
        while matches!(self.current_token, Token::Amp) {
            self.advance();
            let right = self.parse_shift();
            left = ExprAST::Binary {
                op: BinaryOp::BitAnd,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_shift(&mut self) -> ExprAST {
        let mut left = self.parse_term();
        while matches!(self.current_token, Token::Shl | Token::Shr) {
            let tok = self.take_token();
            let op = match tok {
                Token::Shl => BinaryOp::Shl,
                Token::Shr => BinaryOp::Shr,
                _ => unreachable!(),
            };
            let right = self.parse_term();
            left = ExprAST::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_term(&mut self) -> ExprAST {
        let mut left = self.parse_factor();
        while matches!(self.current_token, Token::Plus | Token::Minus) {
            let tok = self.take_token();
            let op = match tok {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => unreachable!(),
            };
            let right = self.parse_factor();
            left = ExprAST::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_factor(&mut self) -> ExprAST {
        let mut left = self.parse_unary();
        while matches!(self.current_token, Token::Star | Token::Slash | Token::Percent) {
            let tok = self.take_token();
            let op = match tok {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Mod,
                _ => unreachable!(),
            };
            let right = self.parse_unary();
            left = ExprAST::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    fn parse_unary(&mut self) -> ExprAST {
        match &self.current_token {
            Token::Bang => {
                self.advance();
                let operand = self.parse_unary();
                ExprAST::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                }
            }
            Token::Plus => {
                self.advance();
                self.parse_unary()
            }
            Token::Minus => {
                self.advance();
                let operand = self.parse_unary();
                ExprAST::Unary {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                }
            }
            Token::Tilde => {
                self.advance();
                let operand = self.parse_unary();
                ExprAST::Unary {
                    op: UnaryOp::BitNot,
                    operand: Box::new(operand),
                }
            }
            Token::Identifier(s) if s == "not" => {
                self.advance();
                let operand = self.parse_unary();
                ExprAST::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                }
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> ExprAST {
        let mut node = match self.take_token() {
            Token::Number(n) => ExprAST::Number(n),
            Token::Float(f) => ExprAST::Float(f),
            Token::String(s) => ExprAST::Str(s),
            Token::Identifier(id) => {
                if id == "true" {
                    ExprAST::Bool(true)
                } else if id == "false" {
                    ExprAST::Bool(false)
                } else {
                    ExprAST::Identifier(id)
                }
            }
            Token::LParen => {
                let inner = self.parse_expression();
                if self.current_token == Token::RParen {
                    self.advance();
                }
                inner
            }
            Token::LBracket => {
                let mut elements = Vec::new();
                if self.current_token != Token::RBracket && self.current_token != Token::EOF {
                    loop {
                        elements.push(self.parse_expression());
                        if self.current_token == Token::Comma {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                if self.current_token == Token::RBracket {
                    self.advance();
                }
                ExprAST::Array(elements)
            }
            _ => ExprAST::Number(0),
        };

        loop {
            if self.current_token == Token::Dot || self.current_token == Token::ColonColon {
                let is_enum = self.current_token == Token::ColonColon;
                self.advance();
                if let Token::Identifier(member) = &self.current_token {
                    let member = member.clone();
                    self.advance();
                    if self.current_token == Token::LParen {
                        self.advance();
                        let mut args = Vec::new();
                        if self.current_token != Token::RParen {
                            loop {
                                args.push(self.parse_expression());
                                if self.current_token == Token::Comma {
                                    self.advance();
                                } else {
                                    break;
                                }
                            }
                        }
                        if self.current_token == Token::RParen {
                            self.advance();
                        }
                        node = ExprAST::MethodCall {
                            base: Box::new(node),
                            method: member,
                            args,
                        };
                    } else {
                        node = ExprAST::MemberAccess {
                            base: Box::new(node),
                            member,
                            is_enum,
                        };
                    }
                } else {
                    break;
                }
            } else if self.current_token == Token::LBracket {
                self.advance();
                let index = self.parse_expression();
                if self.current_token == Token::RBracket {
                    self.advance();
                }
                node = ExprAST::IndexAccess {
                    base: Box::new(node),
                    index: Box::new(index),
                };
            } else {
                break;
            }
        }

        node
    }
}
