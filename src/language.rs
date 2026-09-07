use crate::token::TokenKind;

macro_rules! binary_ops {
    (
        $(
            $name:ident => $token:ident => $cpp:literal
        ),* $(,)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum BinaryOp {
            $(
                $name,
            )*
        }

        impl BinaryOp {
            pub fn from_token(token: &TokenKind) -> Option<Self> {
                match token {
                    $(
                        TokenKind::$token => Some(Self::$name),
                    )*
                    _ => None,
                }
            }

            pub fn as_cpp(self) -> &'static str {
                match self {
                    $(
                        Self::$name => $cpp,
                    )*
                }
            }
        }
    };
}

binary_ops! {
    Add            => Plus          => "+",
    Subtract       => Minus         => "-",
    Multiply       => Star          => "*",
    Divide         => Slash        => "/",
    Modulo         => Percent      => "%",
    And            => And           => "&&",
    Or             => Or            => "||",

    Equal          => EqualEqual   => "==",
    NotEqual       => BangEqual    => "!=",

    Less           => Less         => "<",
    LessEqual      => LessEqual    => "<=",
    Greater        => Greater      => ">",
    GreaterEqual   => GreaterEqual => ">=",

    Assign         => Equal        => "=",
    AddAssign      => PlusEqual    => "+=",
    SubtractAssign => MinusEqual   => "-=",
    MultiplyAssign => StarEqual    => "*=",
    DivideAssign   => SlashEqual   => "/=",
}
