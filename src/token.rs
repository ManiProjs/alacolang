use miette::SourceSpan;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Import,

    // Keywords
    Fn,
    Let,
    Mut,
    Return,
    Stop,
    Skip,
    Loop,
    Repeat,
    While,
    For,
    In,
    If,
    Else,
    True,
    False,
    Match,

    // Identifiers and literals
    Identifier(String),
    Integer(i64),
    Float(f64),
    String(String),

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,

    Equal,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,

    EqualEqual,
    Bang,
    BangEqual,

    Less,
    LessEqual,
    Greater,
    GreaterEqual,

    Arrow,

    // Punctuation
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    Comma,
    Colon,
    Semicolon,
    Dot,

    // Layout
    Newline,

    // End
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub span: SourceSpan,
}

impl Token {
    pub fn new(kind: TokenKind, lexeme: impl Into<String>, start: usize, length: usize) -> Self {
        Self {
            kind,
            lexeme: lexeme.into(),
            span: SourceSpan::new(start.into(), length.into()),
        }
    }
}
