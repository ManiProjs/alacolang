use crate::{
    ast::*,
    token::{Token, TokenKind},
};

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, current: 0 }
    }

    pub fn parse(&mut self) -> Result<Program, String> {
        let mut items = Vec::new();

        self.skip_newlines();

        while !self.check(&TokenKind::Eof) {
            if !self.match_kind(&TokenKind::Fn) {
                return Err(self.error("top-level statements are not allowed; expected 'fn'"));
            }

            items.push(Item::Function(self.parse_function()?));

            self.skip_newlines();
        }

        Ok(Program { items })
    }

    fn parse_item(&mut self) -> Result<Item, String> {
        if self.match_kind(&TokenKind::Fn) {
            return Ok(Item::Function(self.parse_function()?));
        }

        Ok(Item::Statement(self.parse_statement()?))
    }

    fn parse_function(&mut self) -> Result<Function, String> {
        let name = self.expect_identifier("expected function name")?;

        self.expect(&TokenKind::LeftParen, "expected '(' after function name")?;

        let mut params = Vec::new();

        if !self.check(&TokenKind::RightParen) {
            loop {
                let param_name = self.expect_identifier("expected parameter name")?;

                self.expect(&TokenKind::Colon, "expected ':' after parameter name")?;

                let ty = self.parse_type()?;

                params.push(Param {
                    name: param_name,
                    ty,
                });

                if !self.match_kind(&TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect(&TokenKind::RightParen, "expected ')' after parameters")?;

        let return_type = if self.match_kind(&TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        let body = self.parse_block()?;

        Ok(Function {
            name,
            params,
            return_type,
            body,
        })
    }

    fn parse_statement(&mut self) -> Result<Stmt, String> {
        self.skip_newlines();

        if self.match_kind(&TokenKind::Let) {
            return self.parse_let();
        }

        if self.match_kind(&TokenKind::Return) {
            if self.check(&TokenKind::RightBrace) || self.check(&TokenKind::Newline) {
                return Ok(Stmt::Return(None));
            }

            return Ok(Stmt::Return(Some(self.parse_expression()?)));
        }

        if self.match_kind(&TokenKind::Stop) {
            if self.check(&TokenKind::RightBrace) || self.check(&TokenKind::Newline) {
                return Ok(Stmt::Stop(None));
            }

            return Ok(Stmt::Stop(Some(self.parse_expression()?)));
        }

        if self.match_kind(&TokenKind::Skip) {
            return Ok(Stmt::Skip);
        }

        if self.match_kind(&TokenKind::If) {
            return self.parse_if();
        }

        if self.match_kind(&TokenKind::Loop) {
            return self.parse_loop();
        }

        Ok(Stmt::Expr(self.parse_expression()?))
    }

    fn parse_let(&mut self) -> Result<Stmt, String> {
        let mutable = self.match_kind(&TokenKind::Mut);

        let name = self.expect_identifier("expected variable name")?;

        let ty = if self.match_kind(&TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(&TokenKind::Equal, "expected '=' after variable declaration")?;

        let value = self.parse_expression()?;

        Ok(Stmt::Let {
            name,
            mutable,
            ty,
            value,
        })
    }

    fn parse_if(&mut self) -> Result<Stmt, String> {
        let condition = self.parse_expression()?;
        let then_block = self.parse_block()?;

        self.skip_newlines();

        let else_block = if self.match_kind(&TokenKind::Else) {
            Some(self.parse_block()?)
        } else {
            None
        };

        Ok(Stmt::If {
            condition,
            then_block,
            else_block,
        })
    }

    fn parse_loop(&mut self) -> Result<Stmt, String> {
        let kind = if self.match_kind(&TokenKind::LeftParen) {
            if self.match_kind(&TokenKind::Repeat) {
                let expression = self.parse_expression()?;

                self.expect(
                    &TokenKind::RightParen,
                    "expected ')' after repeat expression",
                )?;

                LoopKind::Repeat(expression)
            } else if self.match_kind(&TokenKind::While) {
                let condition = self.parse_expression()?;

                self.expect(&TokenKind::RightParen, "expected ')' after while condition")?;

                LoopKind::While(condition)
            } else if self.match_kind(&TokenKind::For) {
                let variable = self.expect_identifier("expected loop variable")?;

                self.expect(&TokenKind::In, "expected 'in' after loop variable")?;

                let iterable = self.parse_expression()?;

                self.expect(&TokenKind::RightParen, "expected ')' after for loop")?;

                LoopKind::For { variable, iterable }
            } else {
                return Err(self.error("expected repeat, while, or for"));
            }
        } else {
            LoopKind::Infinite
        };

        let binding = if self.match_kind(&TokenKind::Pipe) {
            let name = self.expect_identifier("expected iteration variable")?;

            self.expect(&TokenKind::Pipe, "expected '|' after iteration variable")?;

            Some(name)
        } else {
            None
        };

        let body = self.parse_block()?;

        Ok(Stmt::Loop {
            kind,
            binding,
            body,
        })
    }

    fn parse_block(&mut self) -> Result<Block, String> {
        self.expect(&TokenKind::LeftBrace, "expected '{'")?;

        self.skip_newlines();

        let mut statements = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            statements.push(self.parse_statement()?);
            self.skip_newlines();
        }

        self.expect(&TokenKind::RightBrace, "expected '}'")?;

        Ok(Block { statements })
    }

    fn parse_type(&mut self) -> Result<Type, String> {
        let name = self.expect_identifier("expected type")?;

        Ok(match name.as_str() {
            "Int" => Type::Int,
            "Float" => Type::Float,
            "Bool" => Type::Bool,
            "String" => Type::String,
            _ => Type::Named(name),
        })
    }

    // ------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------

    fn parse_expression(&mut self) -> Result<Expr, String> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, String> {
        let left = self.parse_comparison()?;

        let operator = if self.match_kind(&TokenKind::Equal) {
            Some(BinaryOp::Assign)
        } else if self.match_kind(&TokenKind::PlusEqual) {
            Some(BinaryOp::AddAssign)
        } else if self.match_kind(&TokenKind::MinusEqual) {
            Some(BinaryOp::SubtractAssign)
        } else if self.match_kind(&TokenKind::StarEqual) {
            Some(BinaryOp::MultiplyAssign)
        } else if self.match_kind(&TokenKind::SlashEqual) {
            Some(BinaryOp::DivideAssign)
        } else {
            None
        };

        if let Some(operator) = operator {
            let right = self.parse_assignment()?;

            return Ok(Expr::Binary {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            });
        }

        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_term()?;

        loop {
            let operator = if self.match_kind(&TokenKind::EqualEqual) {
                BinaryOp::Equal
            } else if self.match_kind(&TokenKind::NotEqual) {
                BinaryOp::NotEqual
            } else if self.match_kind(&TokenKind::Less) {
                BinaryOp::Less
            } else if self.match_kind(&TokenKind::LessEqual) {
                BinaryOp::LessEqual
            } else if self.match_kind(&TokenKind::Greater) {
                BinaryOp::Greater
            } else if self.match_kind(&TokenKind::GreaterEqual) {
                BinaryOp::GreaterEqual
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

    fn parse_term(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_factor()?;

        loop {
            let operator = if self.match_kind(&TokenKind::Plus) {
                BinaryOp::Add
            } else if self.match_kind(&TokenKind::Minus) {
                BinaryOp::Subtract
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

    fn parse_factor(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_unary()?;

        loop {
            let operator = if self.match_kind(&TokenKind::Star) {
                BinaryOp::Multiply
            } else if self.match_kind(&TokenKind::Slash) {
                BinaryOp::Divide
            } else if self.match_kind(&TokenKind::Percent) {
                BinaryOp::Modulo
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

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.match_kind(&TokenKind::Minus) {
            return Ok(Expr::Unary {
                operator: UnaryOp::Negate,
                operand: Box::new(self.parse_unary()?),
            });
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_primary()?;

        loop {
            if self.match_kind(&TokenKind::LeftParen) {
                let mut arguments = Vec::new();

                if !self.check(&TokenKind::RightParen) {
                    loop {
                        arguments.push(self.parse_expression()?);

                        if !self.match_kind(&TokenKind::Comma) {
                            break;
                        }
                    }
                }

                self.expect(&TokenKind::RightParen, "expected ')' after arguments")?;

                expression = Expr::Call {
                    callee: Box::new(expression),
                    arguments,
                };
            } else if self.match_kind(&TokenKind::Dot) {
                let member = self.expect_identifier("expected member name after '.'")?;

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

    fn parse_primary(&mut self) -> Result<Expr, String> {
        let token = self.advance();

        match token.kind {
            TokenKind::Number(value) => Ok(Expr::Number(value)),

            TokenKind::String(value) => Ok(Expr::String(value)),

            TokenKind::True => Ok(Expr::Bool(true)),

            TokenKind::False => Ok(Expr::Bool(false)),

            TokenKind::Identifier(name) => Ok(Expr::Identifier(name)),

            TokenKind::LeftParen => {
                let expression = self.parse_expression()?;

                self.expect(&TokenKind::RightParen, "expected ')'")?;

                Ok(expression)
            }

            _ => Err(format!(
                "unexpected token {:?} at {}:{}",
                token.kind, token.line, token.column
            )),
        }
    }

    // ------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.current].clone();
        self.current += 1;
        token
    }

    fn check(&self, kind: &TokenKind) -> bool {
        &self.tokens[self.current].kind == kind
    }

    fn match_kind(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: &TokenKind, message: &str) -> Result<Token, String> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(self.error(message))
        }
    }

    fn expect_identifier(&mut self, message: &str) -> Result<String, String> {
        match &self.tokens[self.current].kind {
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.advance();
                Ok(name)
            }
            _ => Err(self.error(message)),
        }
    }

    fn skip_newlines(&mut self) {
        while self.match_kind(&TokenKind::Newline) {}
    }

    fn error(&self, message: &str) -> String {
        let token = &self.tokens[self.current];

        format!("{} at {}:{}", message, token.line, token.column)
    }
}
