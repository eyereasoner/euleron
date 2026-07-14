use crate::error::{EuleronError, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    AtPrefix,
    AtBase,
    AtVersion,
    Prefix,
    Base,
    Version,
    Iri(String),
    PName(String),
    Var(String),
    Blank(String),
    String(String),
    StringSingle(String),
    StringLong(String),
    StringLongSingle(String),
    StringLongExtraQuote(String),
    StringLongSingleExtraQuote(String),
    Lang(String),
    Number(String),
    Boolean(bool),
    A,
    Dot,
    Semicolon,
    Comma,
    LBrace,
    RBrace,
    LAnnotation,
    RAnnotation,
    LBracket,
    RBracket,
    LParen,
    RParen,
    LTriple,
    RTriple,
    Arrow,
    BackArrow,
    Reverse,
    HatHat,
    Equals,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub offset: usize,
}

pub fn lex(input: &str) -> Result<Vec<Token>> {
    let mut lx = Lexer { input, pos: 0, tokens: Vec::new() };
    lx.run()?;
    Ok(lx.tokens)
}

struct Lexer<'a> {
    input: &'a str,
    pos: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn run(&mut self) -> Result<()> {
        loop {
            self.skip_ws_and_comments();
            let offset = self.pos;
            let Some(ch) = self.peek() else {
                self.tokens.push(Token { kind: TokenKind::Eof, offset });
                return Ok(());
            };

            match ch {
                '<' if self.starts_with("<<") => {
                    self.bump(); self.bump();
                    self.tokens.push(Token { kind: TokenKind::LTriple, offset });
                }
                '<' if self.starts_with("<=") => {
                    self.bump(); self.bump();
                    self.tokens.push(Token { kind: TokenKind::BackArrow, offset });
                }
                '<' if self.starts_with("<-") => {
                    self.bump(); self.bump();
                    self.tokens.push(Token { kind: TokenKind::Reverse, offset });
                }
                '<' => self.read_iri()?,
                '=' if self.starts_with("=>") => {
                    self.bump(); self.bump();
                    self.tokens.push(Token { kind: TokenKind::Arrow, offset });
                }
                '=' => {
                    self.bump();
                    self.tokens.push(Token { kind: TokenKind::Equals, offset });
                }
                '>' if self.starts_with(">>") => {
                    self.bump(); self.bump();
                    self.tokens.push(Token { kind: TokenKind::RTriple, offset });
                }
                '^' if self.starts_with("^^") => {
                    self.bump(); self.bump();
                    self.tokens.push(Token { kind: TokenKind::HatHat, offset });
                }
                '"' | '\'' => self.read_string()?,
                '.' if self.peek_next().is_some_and(|c| c.is_ascii_digit()) => self.read_number()?,
                '.' => { self.bump(); self.tokens.push(Token { kind: TokenKind::Dot, offset }); }
                ';' => { self.bump(); self.tokens.push(Token { kind: TokenKind::Semicolon, offset }); }
                ',' => { self.bump(); self.tokens.push(Token { kind: TokenKind::Comma, offset }); }
                '{' if self.starts_with("{|") => {
                    self.bump(); self.bump();
                    self.tokens.push(Token { kind: TokenKind::LAnnotation, offset });
                }
                '|' if self.starts_with("|}") => {
                    self.bump(); self.bump();
                    self.tokens.push(Token { kind: TokenKind::RAnnotation, offset });
                }
                '{' => { self.bump(); self.tokens.push(Token { kind: TokenKind::LBrace, offset }); }
                '}' => { self.bump(); self.tokens.push(Token { kind: TokenKind::RBrace, offset }); }
                '[' => { self.bump(); self.tokens.push(Token { kind: TokenKind::LBracket, offset }); }
                ']' => { self.bump(); self.tokens.push(Token { kind: TokenKind::RBracket, offset }); }
                '(' => { self.bump(); self.tokens.push(Token { kind: TokenKind::LParen, offset }); }
                ')' => { self.bump(); self.tokens.push(Token { kind: TokenKind::RParen, offset }); }
                _ if ch.is_ascii_digit() || ((ch == '-' || ch == '+') && self.peek_next().is_some_and(|c| c.is_ascii_digit() || (c == '.' && self.peek_third().is_some_and(|d| d.is_ascii_digit())))) => self.read_number()?,
                _ => self.read_word()?,
            }
        }
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            while self.peek().is_some_and(|c| c.is_whitespace()) { self.bump(); }
            if self.peek() == Some('#') {
                while let Some(c) = self.peek() {
                    self.bump();
                    if c == '\n' { break; }
                }
                continue;
            }
            break;
        }
    }

    fn read_iri(&mut self) -> Result<()> {
        let offset = self.pos;
        self.bump();
        let mut value = String::new();
        while let Some(ch) = self.peek() {
            self.bump();
            if ch == '>' {
                self.tokens.push(Token { kind: TokenKind::Iri(value), offset });
                return Ok(());
            }
            if ch == '\\' {
                let Some(esc) = self.peek() else { return Err(EuleronError::at("unterminated IRI escape", offset)); };
                self.bump();
                if esc == 'u' || esc == 'U' {
                    let count = if esc == 'u' { 4 } else { 8 };
                    let mut hex = String::new();
                    for _ in 0..count {
                        let Some(h) = self.peek() else { return Err(EuleronError::at("unterminated Unicode escape", offset)); };
                        self.bump();
                        hex.push(h);
                    }
                    let code = u32::from_str_radix(&hex, 16).map_err(|_| EuleronError::at("invalid Unicode escape", offset))?;
                    let Some(c) = char::from_u32(code) else { return Err(EuleronError::at("invalid Unicode scalar value", offset)); };
                    if is_forbidden_iri_char(c) { return Err(EuleronError::at("forbidden character in IRI reference", offset)); }
                    value.push(c);
                } else {
                    return Err(EuleronError::at(format!("invalid IRI escape \\{}", esc), offset));
                }
            } else {
                if is_forbidden_iri_char(ch) {
                    return Err(EuleronError::at("forbidden character in IRI reference", offset));
                }
                value.push(ch);
            }
        }
        Err(EuleronError::at("unterminated IRI reference", offset))
    }

    fn read_string(&mut self) -> Result<()> {
        let offset = self.pos;
        let quote = self.bump().unwrap();
        let triple = self.starts_with(&format!("{}{}", quote, quote));
        if triple { self.bump(); self.bump(); }
        let mut value = String::new();
        loop {
            let Some(ch) = self.peek() else { return Err(EuleronError::at("unterminated string literal", offset)); };
            self.bump();
            if ch == quote {
                if triple {
                    // In a long string, a run of N quote characters ends the
                    // string with the final three quotes and contributes the
                    // preceding N-3 quotes to the value.  This handles the N3
                    // quote-edge cases in notation3tests without leaving a
                    // stray short-string token behind.
                    let mut run = 1usize;
                    while self.peek() == Some(quote) {
                        self.bump();
                        run += 1;
                    }
                    if run >= 3 {
                        for _ in 0..(run - 3) { value.push(quote); }
                        let kind = if run > 3 {
                            if quote == '"' { TokenKind::StringLongExtraQuote(value) } else { TokenKind::StringLongSingleExtraQuote(value) }
                        } else if quote == '"' {
                            TokenKind::StringLong(value)
                        } else {
                            TokenKind::StringLongSingle(value)
                        };
                        self.tokens.push(Token { kind, offset });
                        return Ok(());
                    }
                    for _ in 0..run { value.push(quote); }
                } else {
                    let kind = if quote == '"' { TokenKind::String(value) } else { TokenKind::StringSingle(value) };
                    self.tokens.push(Token { kind, offset });
                    return Ok(());
                }
            } else if !triple && matches!(ch, '\n' | '\r') {
                return Err(EuleronError::at("newline in short string literal", offset));
            } else if ch == '\\' {
                let Some(esc) = self.peek() else { return Err(EuleronError::at("unterminated string escape", offset)); };
                self.bump();
                match esc {
                    'n' => value.push('\n'),
                    'r' => value.push('\r'),
                    't' => value.push('\t'),
                    'b' => value.push('\u{0008}'),
                    'f' => value.push('\u{000C}'),
                    '"' => value.push('"'),
                    '\'' => value.push('\''),
                    '\\' => value.push('\\'),
                    'u' | 'U' => {
                        let count = if esc == 'u' { 4 } else { 8 };
                        let mut hex = String::new();
                        for _ in 0..count {
                            let Some(h) = self.peek() else { return Err(EuleronError::at("unterminated Unicode escape", offset)); };
                            self.bump();
                            hex.push(h);
                        }
                        let code = u32::from_str_radix(&hex, 16).map_err(|_| EuleronError::at("invalid Unicode escape", offset))?;
                        let Some(c) = char::from_u32(code) else { return Err(EuleronError::at("invalid Unicode scalar value", offset)); };
                        if is_forbidden_string_char(c) { return Err(EuleronError::at("forbidden character in string literal", offset)); }
                        value.push(c);
                    }
                    other => return Err(EuleronError::at(format!("invalid string escape \\{}", other), offset)),
                }
            } else {
                value.push(ch);
            }
        }
    }

    fn read_number(&mut self) -> Result<()> {
        let offset = self.pos;
        let mut value = String::new();
        if matches!(self.peek(), Some('-' | '+')) { value.push(self.bump().unwrap()); }
        while self.peek().is_some_and(|c| c.is_ascii_digit()) { value.push(self.bump().unwrap()); }
        if self.peek() == Some('.') {
            // Accept .5 and 4.e2, but do not consume the statement dot in
            // ordinary terms such as `55.`.
            let after_dot_can_belong_to_number = self.peek_next()
                .is_some_and(|c| c.is_ascii_digit() || matches!(c, 'e' | 'E'));
            let leading_dot_number = value.is_empty() || value == "-" || value == "+";
            if after_dot_can_belong_to_number || leading_dot_number {
                value.push(self.bump().unwrap());
                while self.peek().is_some_and(|c| c.is_ascii_digit()) { value.push(self.bump().unwrap()); }
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            value.push(self.bump().unwrap());
            if matches!(self.peek(), Some('+' | '-')) { value.push(self.bump().unwrap()); }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) { value.push(self.bump().unwrap()); }
        }
        if self.peek() == Some('.') && self.peek_next().is_some_and(|c| c.is_ascii_digit()) {
            return Err(EuleronError::at("malformed numeric literal", offset));
        }
        self.tokens.push(Token { kind: TokenKind::Number(value), offset });
        Ok(())
    }

    fn read_word(&mut self) -> Result<()> {
        let offset = self.pos;
        let mut word = String::new();
        while let Some(ch) = self.peek() {
            if ch == '\\' {
                word.push(ch);
                self.bump();
                if let Some(next) = self.peek() {
                    word.push(next);
                    self.bump();
                }
                continue;
            }
            if ch.is_whitespace() || matches!(ch, '{' | '}' | '[' | ']' | '(' | ')' | ',' | ';' | '|' | '"' | '\'') { break; }
            if ch == '#' { break; }
            if word.starts_with("_:") && word.len() > 2 && ch == ':' { break; }
            if ch == '.' {
                match self.peek_next() {
                    None => break,
                    Some(next) if next.is_whitespace() || matches!(next, '{' | '}' | '[' | ']' | '(' | ')' | ',' | ';' | '|') => break,
                    _ => {}
                }
            }
            if ch == '<' || ch == '>' || ch == '=' { break; }
            word.push(ch);
            self.bump();
        }
        if word.is_empty() {
            return Err(EuleronError::at(format!("unexpected character {:?}", self.peek()), offset));
        }
        let lower = word.to_ascii_lowercase();
        if word.starts_with("@prefix") && word.len() > "@prefix".len() {
            let suffix = word["@prefix".len()..].to_string();
            self.tokens.push(Token { kind: TokenKind::AtPrefix, offset });
            self.tokens.push(Token { kind: TokenKind::PName(suffix), offset: offset + "@prefix".len() });
            return Ok(());
        }
        let kind = match lower.as_str() {
            _ if word == "@prefix" => TokenKind::AtPrefix,
            _ if word == "@base" => TokenKind::AtBase,
            _ if word == "@version" => TokenKind::AtVersion,
            "prefix" => TokenKind::Prefix,
            "base" => TokenKind::Base,
            "version" => TokenKind::Version,
            _ if word == "a" => TokenKind::A,
            "true" => TokenKind::Boolean(true),
            "false" => TokenKind::Boolean(false),
            _ if word.starts_with('@') => TokenKind::Lang(word[1..].to_string()),
            _ if word.starts_with('?') => TokenKind::Var(word[1..].to_string()),
            _ if word.starts_with("_:") => TokenKind::Blank(word[2..].to_string()),
            _ if word.contains(':') => TokenKind::PName(word),
            _ => TokenKind::PName(word),
        };
        self.tokens.push(Token { kind, offset });
        Ok(())
    }

    fn starts_with(&self, s: &str) -> bool { self.input[self.pos..].starts_with(s) }

    fn peek(&self) -> Option<char> { self.input[self.pos..].chars().next() }

    fn peek_next(&self) -> Option<char> {
        let mut it = self.input[self.pos..].chars();
        it.next()?;
        it.next()
    }

    fn peek_third(&self) -> Option<char> {
        let mut it = self.input[self.pos..].chars();
        it.next()?;
        it.next()?;
        it.next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }
}

fn is_forbidden_iri_char(ch: char) -> bool {
    ch.is_control() || ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '\\')
}

fn is_forbidden_string_char(_ch: char) -> bool { false }
