use std::sync::Arc;

use miette::NamedSource;

use crate::{
    ast::{
        Block, Expr, Function, Item, LoopKind, MatchArm, MatchPattern, Param, Program, Stmt, Type,
        UnaryOp,
    },
    error::AlacoError,
    language::BinaryOp,
    token::{Token, TokenKind},
};

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
    source: Arc<NamedSource<String>>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, filename: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            tokens,
            current: 0,
            source: Arc::new(NamedSource::new(filename.into(), source.into())),
        }
    }

    pub fn parse(&mut self) -> Result<Program, AlacoError> {
        let mut items = Vec::new();
        self.skip_newlines();

        while !self.is_at_end() {
            if self.check(&TokenKind::Fn) {
                items.push(Item::Function(self.parse_function()?));
            } else {
                return Err(self.error("top-level statements are not allowed; expected 'fn'"));
            }

            self.skip_newlines();
        }

        Ok(Program { items })
    }

    fn parse_function(&mut self) -> Result<Function, AlacoError> {
        self.consume(&TokenKind::Fn, "expected 'fn'")?;

        let name = self.consume_identifier("expected function name")?;

        self.consume(&TokenKind::LeftParen, "expected '(' after function name")?;

        let mut params = Vec::new();

        if !self.check(&TokenKind::RightParen) {
            loop {
                let param_name = self.consume_identifier("expected parameter name")?;

                self.consume(&TokenKind::Colon, "expected ':' after parameter name")?;

                let ty = self.parse_type()?;

                params.push(Param {
                    name: param_name,
                    ty,
                });

                if !self.matches(&TokenKind::Comma) {
                    break;
                }
            }
        }

        self.consume(&TokenKind::RightParen, "expected ')' after parameters")?;

        let return_type = if self.matches(&TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.skip_newlines();

        let body = self.parse_block()?;

        Ok(Function {
            name,
            params,
            return_type,
            body,
        })
    }

    fn parse_type(&mut self) -> Result<Type, AlacoError> {
        let name = self.consume_identifier("expected type name")?;

        Ok(match name.as_str() {
            "int" => Type::Int,
            "float" => Type::Float,
            "bool" => Type::Bool,
            "string" => Type::String,
            _ => Type::Named(name),
        })
    }

    fn parse_block(&mut self) -> Result<Block, AlacoError> {
        self.consume(&TokenKind::LeftBrace, "expected '{' to start block")?;

        let mut statements = Vec::new();

        self.skip_newlines();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            statements.push(self.parse_statement()?);

            self.consume_statement_end()?;
            self.skip_newlines();
        }

        self.consume(&TokenKind::RightBrace, "expected '}' after block")?;

        Ok(Block { statements })
    }

    fn parse_statement(&mut self) -> Result<Stmt, AlacoError> {
        match self.peek().kind.clone() {
            TokenKind::Let => self.parse_let_statement(),
            TokenKind::Return => self.parse_return_statement(),
            TokenKind::Stop => self.parse_stop_statement(),
            TokenKind::Skip => {
                self.advance();
                Ok(Stmt::Skip)
            }
            TokenKind::Loop => self.parse_loop_statement(),
            TokenKind::If => self.parse_if_statement(),
            TokenKind::Match => self.parse_match_statement(),
            _ => {
                let expression = self.parse_expression()?;
                Ok(Stmt::Expr(expression))
            }
        }
    }

    fn parse_let_statement(&mut self) -> Result<Stmt, AlacoError> {
        self.consume(&TokenKind::Let, "expected 'let'")?;

        let mutable = self.matches(&TokenKind::Mut);

        let name = self.consume_identifier("expected variable name after 'let'")?;

        let ty = if self.matches(&TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.consume(&TokenKind::Equal, "expected '=' after variable declaration")?;

        let value = self.parse_expression()?;

        Ok(Stmt::Let {
            name,
            mutable,
            ty,
            value,
        })
    }

    fn parse_return_statement(&mut self) -> Result<Stmt, AlacoError> {
        self.consume(&TokenKind::Return, "expected 'return'")?;

        if self.is_statement_end() {
            Ok(Stmt::Return(None))
        } else {
            Ok(Stmt::Return(Some(self.parse_expression()?)))
        }
    }

    fn parse_stop_statement(&mut self) -> Result<Stmt, AlacoError> {
        self.consume(&TokenKind::Stop, "expected 'stop'")?;

        if self.is_statement_end() {
            Ok(Stmt::Stop(None))
        } else {
            Ok(Stmt::Stop(Some(self.parse_expression()?)))
        }
    }

    fn parse_if_statement(&mut self) -> Result<Stmt, AlacoError> {
        self.consume(&TokenKind::If, "expected 'if'")?;

        let condition = self.parse_expression()?;

        self.skip_newlines();

        let then_block = self.parse_block()?;

        self.skip_newlines();

        let else_block = if self.matches(&TokenKind::Else) {
            self.skip_newlines();

            if self.check(&TokenKind::If) {
                let nested_if = self.parse_if_statement()?;

                Some(Block {
                    statements: vec![nested_if],
                })
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };

        Ok(Stmt::If {
            condition,
            then_block,
            else_block,
        })
    }

    fn parse_match_statement(&mut self) -> Result<Stmt, AlacoError> {
        self.consume(&TokenKind::Match, "expected 'match'")?;

        let expression = self.parse_expression()?;

        self.skip_newlines();

        self.consume(&TokenKind::LeftBrace, "expected '{' after match expression")?;

        self.skip_newlines();

        let mut arms = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let pattern = self.parse_match_pattern()?;

            self.consume(&TokenKind::Arrow, "expected '=>' after match pattern")?;

            self.skip_newlines();

            let body = if self.check(&TokenKind::LeftBrace) {
                // Full block form:
                //
                // 0 => {
                //     print("zero")
                // }
                self.parse_block()?
            } else {
                // Compact form:
                //
                // 0 => print("zero")
                let statement = self.parse_statement()?;

                self.consume_statement_end()?;
                self.skip_newlines();

                Block {
                    statements: vec![statement],
                }
            };

            arms.push(MatchArm { pattern, body });

            self.skip_newlines();
        }

        self.consume(&TokenKind::RightBrace, "expected '}' after match arms")?;

        Ok(Stmt::Match { expression, arms })
    }

    fn parse_match_pattern(&mut self) -> Result<MatchPattern, AlacoError> {
        match self.peek().kind.clone() {
            TokenKind::Integer(value) => {
                self.advance();
                Ok(MatchPattern::Number(value.to_string()))
            }

            TokenKind::Float(value) => {
                self.advance();
                Ok(MatchPattern::Number(value.to_string()))
            }

            TokenKind::String(value) => {
                self.advance();
                Ok(MatchPattern::String(value))
            }

            TokenKind::True => {
                self.advance();
                Ok(MatchPattern::Bool(true))
            }

            TokenKind::False => {
                self.advance();
                Ok(MatchPattern::Bool(false))
            }

            TokenKind::Identifier(name) => {
                self.advance();

                if name == "_" {
                    Ok(MatchPattern::Wildcard)
                } else {
                    Ok(MatchPattern::Identifier(name))
                }
            }

            _ => Err(self.error("expected match pattern")),
        }
    }

    fn parse_loop_statement(&mut self) -> Result<Stmt, AlacoError> {
        self.consume(&TokenKind::Loop, "expected 'loop'")?;

        let kind = if self.matches(&TokenKind::LeftParen) {
            let kind = if self.matches(&TokenKind::Repeat) {
                LoopKind::Repeat(self.parse_expression()?)
            } else if self.matches(&TokenKind::While) {
                LoopKind::While(self.parse_expression()?)
            } else {
                return Err(self.error("expected 'repeat' or 'while' inside loop parentheses"));
            };

            self.consume(&TokenKind::RightParen, "expected ')' after loop condition")?;

            kind
        } else {
            LoopKind::Infinite
        };

        self.skip_newlines();

        let body = self.parse_block()?;

        Ok(Stmt::Loop {
            kind,
            binding: None,
            body,
        })
    }

    fn parse_expression(&mut self) -> Result<Expr, AlacoError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, AlacoError> {
        let expression = self.parse_equality()?;

        let operator = if self.matches(&TokenKind::Equal) {
            Some(BinaryOp::Assign)
        } else if self.matches(&TokenKind::PlusEqual) {
            Some(BinaryOp::AddAssign)
        } else if self.matches(&TokenKind::MinusEqual) {
            Some(BinaryOp::SubtractAssign)
        } else if self.matches(&TokenKind::StarEqual) {
            Some(BinaryOp::MultiplyAssign)
        } else if self.matches(&TokenKind::SlashEqual) {
            Some(BinaryOp::DivideAssign)
        } else {
            None
        };

        if let Some(operator) = operator {
            let value = self.parse_assignment()?;

            Ok(Expr::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(value),
            })
        } else {
            Ok(expression)
        }
    }

    fn parse_equality(&mut self) -> Result<Expr, AlacoError> {
        let mut expression = self.parse_comparison()?;

        loop {
            let operator = if let Some(operator) = BinaryOp::from_token(&self.peek().kind) {
                if matches!(operator, BinaryOp::Equal | BinaryOp::NotEqual) {
                    self.advance();
                    operator
                } else {
                    break;
                }
            } else {
                break;
            };

            let right = self.parse_comparison()?;

            expression = Expr::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_comparison(&mut self) -> Result<Expr, AlacoError> {
        let mut expression = self.parse_term()?;

        loop {
            let operator = if let Some(operator) = BinaryOp::from_token(&self.peek().kind) {
                if matches!(
                    operator,
                    BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                ) {
                    self.advance();
                    operator
                } else {
                    break;
                }
            } else {
                break;
            };

            let right = self.parse_term()?;

            expression = Expr::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_term(&mut self) -> Result<Expr, AlacoError> {
        let mut expression = self.parse_factor()?;

        loop {
            let operator = if let Some(operator) = BinaryOp::from_token(&self.peek().kind) {
                if matches!(operator, BinaryOp::Add | BinaryOp::Subtract) {
                    self.advance();
                    operator
                } else {
                    break;
                }
            } else {
                break;
            };

            let right = self.parse_factor()?;

            expression = Expr::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_factor(&mut self) -> Result<Expr, AlacoError> {
        let mut expression = self.parse_unary()?;

        loop {
            let operator = if let Some(operator) = BinaryOp::from_token(&self.peek().kind) {
                if matches!(
                    operator,
                    BinaryOp::Multiply | BinaryOp::Divide | BinaryOp::Modulo
                ) {
                    self.advance();
                    operator
                } else {
                    break;
                }
            } else {
                break;
            };

            let right = self.parse_unary()?;

            expression = Expr::Binary {
                left: Box::new(expression),
                operator,
                right: Box::new(right),
            };
        }

        Ok(expression)
    }

    fn parse_unary(&mut self) -> Result<Expr, AlacoError> {
        if self.matches(&TokenKind::Minus) {
            let operand = self.parse_unary()?;

            return Ok(Expr::Unary {
                operator: UnaryOp::Negate,
                operand: Box::new(operand),
            });
        }

        self.parse_call()
    }

    fn parse_call(&mut self) -> Result<Expr, AlacoError> {
        let mut expression = self.parse_primary()?;

        loop {
            if self.matches(&TokenKind::LeftParen) {
                let mut arguments = Vec::new();

                if !self.check(&TokenKind::RightParen) {
                    loop {
                        arguments.push(self.parse_expression()?);

                        if !self.matches(&TokenKind::Comma) {
                            break;
                        }
                    }
                }

                self.consume(
                    &TokenKind::RightParen,
                    "expected ')' after function arguments",
                )?;

                expression = Expr::Call {
                    callee: Box::new(expression),
                    arguments,
                };
            } else if self.matches(&TokenKind::Dot) {
                let member = self.consume_identifier("expected member name after '.'")?;

                expression = Expr::Member {
                    object: Box::new(expression),
                    member,
                };
            } else {
                break;
            }
        }

        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<Expr, AlacoError> {
        match self.peek().kind.clone() {
            TokenKind::Integer(value) => {
                self.advance();
                Ok(Expr::Number(value.to_string()))
            }

            TokenKind::Float(value) => {
                self.advance();
                Ok(Expr::Number(value.to_string()))
            }

            TokenKind::String(value) => {
                self.advance();
                Ok(Expr::String(value))
            }

            TokenKind::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }

            TokenKind::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }

            TokenKind::Identifier(name) => {
                self.advance();
                Ok(Expr::Identifier(name))
            }

            TokenKind::LeftParen => {
                self.advance();

                let expression = self.parse_expression()?;

                self.consume(&TokenKind::RightParen, "expected ')' after expression")?;

                Ok(expression)
            }

            _ => Err(self.error("expected expression")),
        }
    }

    fn consume_statement_end(&mut self) -> Result<(), AlacoError> {
        if self.matches(&TokenKind::Semicolon) {
            self.skip_newlines();
            return Ok(());
        }

        if self.matches(&TokenKind::Newline) {
            self.skip_newlines();
            return Ok(());
        }

        if self.check(&TokenKind::RightBrace) || self.check(&TokenKind::Eof) {
            return Ok(());
        }

        Err(self.error("expected newline, ';', or '}' after statement"))
    }

    fn is_statement_end(&self) -> bool {
        self.check(&TokenKind::Newline)
            || self.check(&TokenKind::Semicolon)
            || self.check(&TokenKind::RightBrace)
            || self.check(&TokenKind::Eof)
    }

    fn consume_identifier(&mut self, message: &str) -> Result<String, AlacoError> {
        match self.peek().kind.clone() {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(name)
            }

            _ => Err(self.error(message)),
        }
    }

    fn consume(&mut self, kind: &TokenKind, message: &str) -> Result<(), AlacoError> {
        if self.check(kind) {
            self.advance();
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn matches(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check(&self, kind: &TokenKind) -> bool {
        if self.is_at_end() {
            return *kind == TokenKind::Eof;
        }

        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind)
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }

        self.previous()
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }

    fn skip_newlines(&mut self) {
        while self.matches(&TokenKind::Newline) {}
    }

    fn error(&self, expected: impl Into<String>) -> AlacoError {
        let token = self.peek();

        AlacoError::UnexpectedToken {
            expected: expected.into(),
            found: token.lexeme.clone(),
            span: token.span,
        }
    }
}
