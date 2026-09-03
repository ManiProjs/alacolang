use miette::SourceSpan;

use crate::{
    error::AlacoError,
    token::{Token, TokenKind},
};

pub struct Lexer<'a> {
    source: &'a str,
    chars: std::str::CharIndices<'a>,
    current: usize,
    start: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.char_indices(),
            current: 0,
            start: 0,
            tokens: Vec::new(),
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, AlacoError> {
        while let Some((index, ch)) = self.chars.next() {
            self.current = index + ch.len_utf8();
            self.start = index;

            self.scan_token(ch)?;
        }

        self.current = self.source.len();

        self.tokens
            .push(Token::new(TokenKind::Eof, "", self.current, 0));

        Ok(std::mem::take(&mut self.tokens))
    }

    fn scan_token(&mut self, ch: char) -> Result<(), AlacoError> {
        match ch {
            // Whitespace
            ' ' | '\t' | '\r' => {}

            // Newline
            '\n' => {
                self.add_token(TokenKind::Newline);
            }

            // Single-character tokens
            '(' => self.add_token(TokenKind::LeftParen),
            ')' => self.add_token(TokenKind::RightParen),
            '{' => self.add_token(TokenKind::LeftBrace),
            '}' => self.add_token(TokenKind::RightBrace),
            ',' => self.add_token(TokenKind::Comma),
            ':' => self.add_token(TokenKind::Colon),
            ';' => self.add_token(TokenKind::Semicolon),
            '.' => self.add_token(TokenKind::Dot),

            // Operators
            '+' => {
                if self.match_char('=') {
                    self.add_token(TokenKind::PlusEqual);
                } else {
                    self.add_token(TokenKind::Plus);
                }
            }

            '-' => {
                if self.match_char('=') {
                    self.add_token(TokenKind::MinusEqual);
                } else if self.match_char('>') {
                    self.add_token(TokenKind::Arrow);
                } else {
                    self.add_token(TokenKind::Minus);
                }
            }

            '*' => {
                if self.match_char('=') {
                    self.add_token(TokenKind::StarEqual);
                } else {
                    self.add_token(TokenKind::Star);
                }
            }

            '/' => {
                if self.match_char('/') {
                    self.skip_line_comment();
                } else if self.match_char('=') {
                    self.add_token(TokenKind::SlashEqual);
                } else {
                    self.add_token(TokenKind::Slash);
                }
            }

            '%' => {
                self.add_token(TokenKind::Percent);
            }

            '=' => {
                if self.match_char('=') {
                    self.add_token(TokenKind::EqualEqual);
                } else {
                    self.add_token(TokenKind::Equal);
                }
            }

            '!' => {
                if self.match_char('=') {
                    self.add_token(TokenKind::BangEqual);
                } else {
                    self.add_token(TokenKind::Bang);
                }
            }

            '<' => {
                if self.match_char('=') {
                    self.add_token(TokenKind::LessEqual);
                } else {
                    self.add_token(TokenKind::Less);
                }
            }

            '>' => {
                if self.match_char('=') {
                    self.add_token(TokenKind::GreaterEqual);
                } else {
                    self.add_token(TokenKind::Greater);
                }
            }

            // String
            '"' => {
                self.scan_string()?;
            }

            // Number
            '0'..='9' => {
                self.scan_number()?;
            }

            // Identifier / keyword
            'a'..='z' | 'A'..='Z' | '_' => {
                self.scan_identifier();
            }

            // Anything else
            _ => {
                return Err(AlacoError::UnexpectedCharacter {
                    character: ch,
                    span: self.span(),
                });
            }
        }

        Ok(())
    }

    fn scan_string(&mut self) -> Result<(), AlacoError> {
        let content_start = self.current;

        let mut escaped = false;

        while let Some((index, ch)) = self.chars.next() {
            self.current = index + ch.len_utf8();

            if escaped {
                escaped = false;
                continue;
            }

            if ch == '\\' {
                escaped = true;
                continue;
            }

            if ch == '"' {
                let raw = &self.source[content_start..index];

                let value = self
                    .unescape_string(raw)
                    .map_err(|_| AlacoError::InvalidEscape {
                        literal: raw.to_string(),
                        span: (content_start..index + 1).into(),
                    })?;

                self.add_token_with_lexeme(TokenKind::String(value), self.current);

                return Ok(());
            }

            if ch == '\n' {
                return Err(AlacoError::UnterminatedString {
                    literal: self.source[self.start..self.current].to_string(),
                    span: self.span(),
                });
            }
        }

        Err(AlacoError::UnterminatedString {
            literal: self.source[self.start..self.current].to_string(),
            span: self.span(),
        })
    }

    fn unescape_string(&self, value: &str) -> Result<String, ()> {
        let mut result = String::with_capacity(value.len());

        let mut chars = value.chars();

        while let Some(ch) = chars.next() {
            if ch != '\\' {
                result.push(ch);
                continue;
            }

            let escaped = chars.next().ok_or(())?;

            match escaped {
                'n' => result.push('\n'),
                'r' => result.push('\r'),
                't' => result.push('\t'),
                '\\' => result.push('\\'),
                '"' => result.push('"'),
                '0' => result.push('\0'),

                _ => return Err(()),
            }
        }

        Ok(result)
    }

    fn scan_number(&mut self) -> Result<(), AlacoError> {
        self.consume_digits();

        let mut is_float = false;

        // Check for decimal part.
        //
        // Only treat '.' as part of the number when it is
        // followed by a digit. This means:
        //
        //     123.foo
        //
        // is tokenized as:
        //
        //     123 . foo
        //
        // instead of an invalid float.
        if self.peek_char() == Some('.')
            && self.peek_next_char().is_some_and(|c| c.is_ascii_digit())
        {
            is_float = true;

            self.advance_char();

            self.consume_digits();
        }

        // Reject things such as:
        //
        //     123abc
        //
        // instead of silently tokenizing them as:
        //
        //     123 abc
        //
        if self
            .peek_char()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            let invalid_start = self.start;

            while self
                .peek_char()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                self.advance_char();
            }

            return Err(AlacoError::InvalidNumber {
                literal: self.source[invalid_start..self.current].to_string(),
                span: (invalid_start..self.current).into(),
            });
        }

        let literal = &self.source[self.start..self.current];

        if is_float {
            let value = literal
                .parse::<f64>()
                .map_err(|_| AlacoError::InvalidFloat {
                    literal: literal.to_string(),
                    span: self.span(),
                })?;

            self.add_token(TokenKind::Float(value));
        } else {
            let value = literal
                .parse::<i64>()
                .map_err(|_| AlacoError::InvalidInteger {
                    literal: literal.to_string(),
                    span: self.span(),
                })?;

            self.add_token(TokenKind::Integer(value));
        }

        Ok(())
    }

    fn scan_identifier(&mut self) {
        while self
            .peek_char()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            self.advance_char();
        }

        let text = &self.source[self.start..self.current];

        let kind = match text {
            "fn" => TokenKind::Fn,
            "let" => TokenKind::Let,
            "mut" => TokenKind::Mut,
            "return" => TokenKind::Return,
            "stop" => TokenKind::Stop,
            "skip" => TokenKind::Skip,
            "loop" => TokenKind::Loop,
            "repeat" => TokenKind::Repeat,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => TokenKind::Identifier(text.to_string()),
        };

        self.add_token(kind);
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch == '\n' {
                break;
            }

            self.advance_char();
        }
    }

    fn consume_digits(&mut self) {
        while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
            self.advance_char();
        }
    }

    fn match_char(&mut self, expected: char) -> bool {
        if self.peek_char() == Some(expected) {
            self.advance_char();
            true
        } else {
            false
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.current..].chars().next()
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut chars = self.source[self.current..].chars();

        chars.next()?;
        chars.next()
    }

    fn advance_char(&mut self) -> Option<char> {
        let (index, ch) = self.chars.next()?;

        self.current = index + ch.len_utf8();

        Some(ch)
    }

    fn add_token(&mut self, kind: TokenKind) {
        self.add_token_with_lexeme(kind, self.current);
    }

    fn add_token_with_lexeme(&mut self, kind: TokenKind, end: usize) {
        let lexeme = self.source[self.start..end].to_string();

        self.tokens
            .push(Token::new(kind, lexeme, self.start, end - self.start));
    }

    fn span(&self) -> SourceSpan {
        SourceSpan::new(
            self.start.into(),
            self.current.saturating_sub(self.start).into(),
        )
    }
}
