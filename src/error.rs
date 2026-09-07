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

    #[error("duplicate struct `{name}`")]
    #[diagnostic(code(alaco::analysis::duplicate_struct))]
    DuplicateStruct {
        name: String,

        #[label("duplicate struct")]
        span: SourceSpan,
    },

    #[error("duplicate field `{field}` in struct `{struct_name}`")]
    #[diagnostic(code(alaco::analysis::duplicate_field))]
    DuplicateField {
        field: String,
        struct_name: String,

        #[label("duplicate field")]
        span: SourceSpan,
    },

    #[error("unknown type `{name}`")]
    #[diagnostic(code(alaco::analysis::unknown_type))]
    UnknownType {
        name: String,

        #[label("unknown type")]
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

    // ─────────────────────────────────────────────────────────────
    // Standard library
    // ─────────────────────────────────────────────────────────────
    #[error("failed to load standard library module `{module}`")]
    #[diagnostic(
        code(alaco::stdlib::load_error),
        help("make sure the Alaco standard library is installed correctly")
    )]
    StdlibLoadError {
        module: String,
        #[source]
        source: std::io::Error,
    },
}

impl AlacoError {
    pub fn explanation(&self) -> Option<(String, String)> {
        match self {
            Self::UnexpectedCharacter { character, .. } => Some((
                "An unexpected character was found while tokenizing the source.".into(),
                format!(
                    "Remove `{character}` or use it as part of a valid Alaco expression."
                ),
            )),

            Self::UnterminatedString { .. } => Some((
                "A string was started but never closed.".into(),
                "Add the missing `\"` at the end of the string.".into(),
            )),

            Self::InvalidFloat { literal, .. } => Some((
                format!("`{literal}` is not a valid floating-point literal."),
                "Check the number's format. A floating-point literal should contain a valid decimal representation.".into(),
            )),

            Self::InvalidInteger { literal, .. } => Some((
                format!("`{literal}` is not a valid integer literal."),
                "Check the number and make sure it contains only a valid integer representation.".into(),
            )),

            Self::InvalidEscape { literal, .. } => Some((
                format!("The string contains an invalid escape sequence: `{literal}`."),
                "Use a valid Alaco escape sequence or remove the backslash.".into(),
            )),

            Self::InvalidNumber { literal, .. } => Some((
                format!("`{literal}` is not a valid number literal."),
                "Check the number's syntax and remove any invalid characters.".into(),
            )),

            Self::UnexpectedToken {
                expected,
                found,
                ..
            } => Some((
                format!(
                    "The parser expected {expected}, but found `{found}` instead."
                ),
                "Check the surrounding syntax and add, remove, or replace the unexpected token.".into(),
            )),

            Self::Analysis { message, .. } => Some((
                message.clone(),
                "Review the highlighted code and make the expression satisfy the compiler's requirements.".into(),
            )),

            Self::UndefinedVariable { name, .. } => Some((
                format!(
                    "`{name}` is being used before it was declared in the current scope."
                ),
                format!("Declare `{name}` before using it."),
            )),

            Self::DuplicateVariable { name, .. } => Some((
                format!(
                    "`{name}` has already been declared in this scope."
                ),
                "Remove the duplicate declaration or use a different variable name.".into(),
            )),

            Self::DuplicateStruct { name, .. } => Some((
                format!("Duplicate struct '{}' found.", name),
                "Ensure that struct names are unique within the program.".into(),
            )),

            Self::DuplicateField { field, struct_name, .. } => Some((
                format!("Field '{}' is already defined in struct '{}'.", field, struct_name),
                "Ensure that each field in a struct has a unique name.".into(),
            )),

            Self::UnknownType { name, .. } => Some((
                format!("Unknown type '{}' used.", name),
                format!("Make sure '{}' is defined as a struct or is a built-in type.", name),
            )),

            Self::ImmutableAssignment { name, .. } => Some((
                format!(
                    "`{name}` was declared as immutable, so it cannot be assigned a new value."
                ),
                format!(
                    "If `{name}` needs to change, declare it with `let mut`."
                ),
            )),

            Self::UndefinedAssignment { name, .. } => Some((
                format!(
                    "`{name}` is being assigned before it has been declared."
                ),
                format!("Declare `{name}` before assigning to it."),
            )),

            Self::InvalidAssignmentTarget { .. } => Some((
                "The left side of an assignment is not something Alaco can assign to."
                    .into(),
                "Assign to a variable or another supported assignable target.".into(),
            )),

            Self::StopOutsideLoop { .. } => Some((
                "`stop` is only valid inside a loop.".into(),
                "Move `stop` into a `for` or other supported loop.".into(),
            )),

            Self::SkipOutsideLoop { .. } => Some((
                "`skip` is only valid inside a loop.".into(),
                "Move `skip` into a `for` or other supported loop.".into(),
            )),

            Self::DuplicateParameter {
                name,
                function,
                ..
            } => Some((
                format!(
                    "The function `{function}` declares the parameter `{name}` more than once."
                ),
                "Give each parameter a unique name.".into(),
            )),

            Self::MissingMain => Some((
                "Every Alaco program needs an entry point named `main`.".into(),
                "Add `fn main() { ... }` to the program.".into(),
            )),

            Self::MultipleMain => Some((
                "An Alaco program can only have one `main` function.".into(),
                "Remove or rename the additional `main` function.".into(),
            )),

            Self::MainHasParameters => Some((
                "`main` is the program entry point and cannot have parameters.".into(),
                "Remove the parameters from `main`.".into(),
            )),

            Self::MainHasReturnType => Some((
                "`main` cannot have an explicit return type.".into(),
                "Remove the return type from `main`.".into(),
            )),

            Self::ForLoopBinding { .. } => Some((
                "This `for` loop already has its iteration binding.".into(),
                "Remove the additional iteration binding.".into(),
            )),

            Self::TopLevelStatement { .. } => Some((
                "This statement appears outside a function.".into(),
                "Move the statement into `main` or another function.".into(),
            )),
            Self::StdlibLoadError { module, .. } => Some((
                format!("The standard library module `{module}` could not be loaded."),
                "Check that the module is installed and that ALACO_STDLIB_DIR points to the correct standard library directory.".into(),
            )),
        }
    }
}
