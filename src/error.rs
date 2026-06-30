use std::fmt;

#[derive(Debug, Clone)]
pub struct EyelingError {
    pub message: String,
    pub offset: Option<usize>,
}

impl EyelingError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), offset: None }
    }

    pub fn at(message: impl Into<String>, offset: usize) -> Self {
        Self { message: message.into(), offset: Some(offset) }
    }

    pub fn with_source_location(&self, source: &str, label: &str) -> String {
        match self.offset {
            None => format!("{}: {}", label, self.message),
            Some(offset) => {
                let (line, col) = line_col(source, offset);
                let line_text = source.lines().nth(line.saturating_sub(1)).unwrap_or("");
                let caret = format!("{}^", " ".repeat(col.saturating_sub(1)));
                format!("{}:{}:{}: {}\n{}\n{}", label, line, col, self.message, line_text, caret)
            }
        }
    }
}

fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in source.char_indices() {
        if i >= offset { break; }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

impl fmt::Display for EyelingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.message) }
}

impl std::error::Error for EyelingError {}

impl From<std::io::Error> for EyelingError {
    fn from(value: std::io::Error) -> Self { Self::new(value.to_string()) }
}

pub type Result<T> = std::result::Result<T, EyelingError>;
