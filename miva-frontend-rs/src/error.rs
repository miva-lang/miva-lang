use std::fmt;

/// A lex/parse failure with the source position where it was detected.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub file: String,
    pub line: i64,
    pub col: i64,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(
                f,
                "{}:{}:{}: {}",
                self.file, self.line, self.col, self.message
            )
        } else {
            write!(f, "{}: {}", self.file, self.message)
        }
    }
}

impl std::error::Error for ParseError {}
