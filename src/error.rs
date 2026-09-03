use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum AlacoError {
    // ─────────────────────────────────────────────────────────────
    // Lexer
    // ─────────────────────────────────────────────────────────────
    #[error("unexpected character `{character}`")]
    #[diagnostic(code(alaco::lexer::unexpected_character))]
    UnexpectedCharacter {
        character: char,

        #[label("unexpected character")]
        span: SourceSpan,
    },

    #[error("unterminated string")]
    #[diagnostic(code(alaco::lexer::unterminated_string))]
    UnterminatedString {
        #[label("string starts here")]
        span: SourceSpan,

        literal: String,
    },

    #[error("invalid float literal `{literal}`")]
    #[diagnostic(code(alaco::lexer::invalid_float))]
    InvalidFloat {
        literal: String,

        #[label("invalid float")]
        span: SourceSpan,
    },

    #[error("invalid integer literal `{literal}`")]
    #[diagnostic(code(alaco::lexer::invalid_integer))]
    InvalidInteger {
        literal: String,

        #[label("invalid integer")]
        span: SourceSpan,
    },

    #[error("invalid escape sequence in string `{literal}`")]
    #[diagnostic(code(alaco::lexer::invalid_escape))]
    InvalidEscape {
        literal: String,

        #[label("invalid escape")]
        span: SourceSpan,
    },

    #[error("invalid number literal `{literal}`")]
    #[diagnostic(code(alaco::lexer::invalid_number))]
    InvalidNumber {
        literal: String,

        #[label("invalid number")]
        span: SourceSpan,
    },

    // ─────────────────────────────────────────────────────────────
    // Parser
    // ─────────────────────────────────────────────────────────────
    #[error("unexpected token")]
    #[diagnostic(code(alaco::parser::unexpected_token), help("expected {expected}"))]
    UnexpectedToken {
        expected: String,
        found: String,

        #[label("found `{found}`")]
        span: SourceSpan,
    },

    // ─────────────────────────────────────────────────────────────
    // Analyzer
    // ─────────────────────────────────────────────────────────────
    #[error("analysis error: {message}")]
    #[diagnostic(code(alaco::analysis::analysis_error))]
    Analysis {
        message: String,

        #[label("analysis error")]
        span: Option<SourceSpan>,
    },

    #[error("undefined variable `{name}`")]
    #[diagnostic(
        code(alaco::analysis::undefined_variable),
        help("declare `{name}` before using it")
    )]
    UndefinedVariable {
        name: String,

        #[label("not defined")]
        span: SourceSpan,
    },

    #[error("variable `{name}` is already declared in this scope")]
    #[diagnostic(code(alaco::analysis::duplicate_variable))]
    DuplicateVariable {
        name: String,

        #[label("already declared here")]
        span: SourceSpan,
    },

    #[error("cannot assign to immutable variable `{name}`")]
    #[diagnostic(
        code(alaco::analysis::immutable_assignment),
        help("declare it using `let mut` if it needs to be changed")
    )]
    ImmutableAssignment {
        name: String,

        #[label("cannot assign to immutable variable")]
        span: SourceSpan,
    },

    #[error("cannot assign to undefined variable `{name}`")]
    #[diagnostic(code(alaco::analysis::undefined_assignment))]
    UndefinedAssignment {
        name: String,

        #[label("not defined")]
        span: SourceSpan,
    },

    #[error("invalid assignment target")]
    #[diagnostic(
        code(alaco::analysis::invalid_assignment_target),
        help("the left side of an assignment must be a variable or member")
    )]
    InvalidAssignmentTarget {
        #[label("not assignable")]
        span: SourceSpan,
    },

    #[error("`stop` can only be used inside a loop")]
    #[diagnostic(code(alaco::analysis::stop_outside_loop))]
    StopOutsideLoop {
        #[label("not inside a loop")]
        span: SourceSpan,
    },

    #[error("`skip` can only be used inside a loop")]
    #[diagnostic(code(alaco::analysis::skip_outside_loop))]
    SkipOutsideLoop {
        #[label("not inside a loop")]
        span: SourceSpan,
    },

    #[error("duplicate parameter `{name}` in function `{function}`")]
    #[diagnostic(code(alaco::analysis::duplicate_parameter))]
    DuplicateParameter {
        name: String,
        function: String,

        #[label("duplicate parameter")]
        span: SourceSpan,
    },

    #[error("duplicate function `{name}`")]
    #[diagnostic(code(alaco::analysis::duplicate_function))]
    DuplicateFunction {
        name: String,

        #[label("duplicate function")]
        span: SourceSpan,
    },

    #[error("program must contain a `main` function")]
    #[diagnostic(code(alaco::analysis::missing_main), help("add `fn main() {{ ... }}`"))]
    MissingMain,

    #[error("program can only contain one `main` function")]
    #[diagnostic(code(alaco::analysis::multiple_main))]
    MultipleMain,

    #[error("`main` function cannot have parameters")]
    #[diagnostic(code(alaco::analysis::main_parameters))]
    MainHasParameters,

    #[error("`main` function cannot have an explicit return type")]
    #[diagnostic(code(alaco::analysis::main_return_type))]
    MainHasReturnType,

    #[error("`for` loop cannot have an additional iteration binding")]
    #[diagnostic(code(alaco::analysis::for_binding))]
    ForLoopBinding {
        #[label("additional binding")]
        span: SourceSpan,
    },

    #[error("top-level statements are not allowed")]
    #[diagnostic(code(alaco::analysis::top_level_statement))]
    TopLevelStatement {
        #[label("statement is not allowed here")]
        span: SourceSpan,
    },
}
