use crate::token::{Token, TokenKind};

pub struct Lexer<'a> {
    source: &'a str,
    chars: std::str::Chars<'a>,
    current: Option<char>,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut chars = source.chars();
        let current = chars.next();

        Self {
            source,
            chars,
            current,
            line: 1,
            column: 1,
        }
    }

    fn advance(&mut self) -> Option<char> {
        let current = self.current;

        if current == Some('\n') {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }

        self.current = self.chars.next();
        current
    }

    fn peek(&self) -> Option<char> {
        self.current
    }

    fn peek_next(&self) -> Option<char> {
        let mut chars = self.source.chars();

        while let Some(c) = chars.next() {
            if Some(c) == self.current {
                return chars.next();
            }
        }

        None
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\r')) {
            self.advance();
        }
    }

    fn identifier(&mut self) -> String {
        let mut value = String::new();

        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                value.push(c);
                self.advance();
            } else {
                break;
            }
        }

        value
    }

    fn number(&mut self) -> String {
        let mut value = String::new();

        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '.' {
                value.push(c);
                self.advance();
            } else {
                break;
            }
        }

        value
    }

    fn string(&mut self) -> Result<String, String> {
        self.advance(); // opening "

        let mut value = String::new();

        while let Some(c) = self.peek() {
            match c {
                '"' => {
                    self.advance();
                    return Ok(value);
                }

                '\\' => {
                    self.advance();

                    match self.peek() {
                        Some('n') => {
                            value.push('\n');
                            self.advance();
                        }
                        Some('t') => {
                            value.push('\t');
                            self.advance();
                        }
                        Some('"') => {
                            value.push('"');
                            self.advance();
                        }
                        Some('\\') => {
                            value.push('\\');
                            self.advance();
                        }
                        Some(other) => {
                            value.push('\\');
                            value.push(other);
                            self.advance();
                        }
                        None => {
                            return Err("unterminated string".into());
                        }
                    }
                }

                '\n' => {
                    return Err(format!("unterminated string at line {}", self.line));
                }

                _ => {
                    value.push(c);
                    self.advance();
                }
            }
        }

        Err("unterminated string".into())
    }

    fn keyword(identifier: &str) -> Option<TokenKind> {
        Some(match identifier {
            "fn" => TokenKind::Fn,
            "let" => TokenKind::Let,
            "mut" => TokenKind::Mut,
            "loop" => TokenKind::Loop,
            "repeat" => TokenKind::Repeat,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "return" => TokenKind::Return,
            "stop" => TokenKind::Stop,
            "skip" => TokenKind::Skip,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => return None,
        })
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();

        while self.peek().is_some() {
            self.skip_whitespace();

            let line = self.line;
            let column = self.column;

            let Some(c) = self.peek() else {
                break;
            };

            let kind = match c {
                '\n' => {
                    self.advance();
                    TokenKind::Newline
                }

                '#' => {
                    while !matches!(self.peek(), Some('\n') | None) {
                        self.advance();
                    }
                    continue;
                }

                '"' => TokenKind::String(self.string()?),

                'a'..='z' | 'A'..='Z' | '_' => {
                    let value = self.identifier();

                    Self::keyword(&value).unwrap_or(TokenKind::Identifier(value))
                }

                '0'..='9' => TokenKind::Number(self.number()),

                '+' => {
                    self.advance();

                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::PlusEqual
                    } else {
                        TokenKind::Plus
                    }
                }

                '-' => {
                    self.advance();

                    if self.peek() == Some('>') {
                        self.advance();
                        TokenKind::Arrow
                    } else if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::MinusEqual
                    } else {
                        TokenKind::Minus
                    }
                }

                '*' => {
                    self.advance();

                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::StarEqual
                    } else {
                        TokenKind::Star
                    }
                }

                '/' => {
                    self.advance();

                    if self.peek() == Some('/') {
                        while !matches!(self.peek(), Some('\n') | None) {
                            self.advance();
                        }
                        continue;
                    }

                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::SlashEqual
                    } else {
                        TokenKind::Slash
                    }
                }

                '%' => {
                    self.advance();
                    TokenKind::Percent
                }

                '=' => {
                    self.advance();

                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::EqualEqual
                    } else {
                        TokenKind::Equal
                    }
                }

                '!' => {
                    self.advance();

                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::NotEqual
                    } else {
                        return Err(format!("unexpected '!' at {}:{}", line, column));
                    }
                }

                '<' => {
                    self.advance();

                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::LessEqual
                    } else {
                        TokenKind::Less
                    }
                }

                '>' => {
                    self.advance();

                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::GreaterEqual
                    } else {
                        TokenKind::Greater
                    }
                }

                '(' => {
                    self.advance();
                    TokenKind::LeftParen
                }

                ')' => {
                    self.advance();
                    TokenKind::RightParen
                }

                '{' => {
                    self.advance();
                    TokenKind::LeftBrace
                }

                '}' => {
                    self.advance();
                    TokenKind::RightBrace
                }

                '[' => {
                    self.advance();
                    TokenKind::LeftBracket
                }

                ']' => {
                    self.advance();
                    TokenKind::RightBracket
                }

                ',' => {
                    self.advance();
                    TokenKind::Comma
                }

                ':' => {
                    self.advance();
                    TokenKind::Colon
                }

                '.' => {
                    self.advance();
                    TokenKind::Dot
                }

                '|' => {
                    self.advance();
                    TokenKind::Pipe
                }

                _ => {
                    return Err(format!(
                        "unexpected character '{}' at {}:{}",
                        c, line, column
                    ));
                }
            };

            tokens.push(Token::new(kind, line, column));
        }

        tokens.push(Token::new(TokenKind::Eof, self.line, self.column));

        Ok(tokens)
    }
}
