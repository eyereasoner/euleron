use crate::error::{EyelingError, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    AtPrefix,
    AtBase,
    Prefix,
    Base,
    Iri(String),
    PName(String),
    Var(String),
    Blank(String),
    String(String),
    Lang(String),
    Number(String),
    Boolean(bool),
    A,
    Dot,
    Semicolon,
    Comma,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Arrow,
    BackArrow,
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
                '<' if self.starts_with("<=") => {
                    self.bump(); self.bump();
                    self.tokens.push(Token { kind: TokenKind::BackArrow, offset });
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
                '^' if self.starts_with("^^") => {
                    self.bump(); self.bump();
                    self.tokens.push(Token { kind: TokenKind::HatHat, offset });
                }
                '"' | '\'' => self.read_string()?,
                '.' => { self.bump(); self.tokens.push(Token { kind: TokenKind::Dot, offset }); }
                ';' => { self.bump(); self.tokens.push(Token { kind: TokenKind::Semicolon, offset }); }
                ',' => { self.bump(); self.tokens.push(Token { kind: TokenKind::Comma, offset }); }
                '{' => { self.bump(); self.tokens.push(Token { kind: TokenKind::LBrace, offset }); }
                '}' => { self.bump(); self.tokens.push(Token { kind: TokenKind::RBrace, offset }); }
                '[' => { self.bump(); self.tokens.push(Token { kind: TokenKind::LBracket, offset }); }
                ']' => { self.bump(); self.tokens.push(Token { kind: TokenKind::RBracket, offset }); }
                '(' => { self.bump(); self.tokens.push(Token { kind: TokenKind::LParen, offset }); }
                ')' => { self.bump(); self.tokens.push(Token { kind: TokenKind::RParen, offset }); }
                _ if ch.is_ascii_digit() || (ch == '-' && self.peek_next().is_some_and(|c| c.is_ascii_digit())) => self.read_number()?,
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
                let Some(esc) = self.peek() else { return Err(EyelingError::at("unterminated IRI escape", offset)); };
                self.bump();
                value.push(esc);
            } else {
                value.push(ch);
            }
        }
        Err(EyelingError::at("unterminated IRI reference", offset))
    }

    fn read_string(&mut self) -> Result<()> {
        let offset = self.pos;
        let quote = self.bump().unwrap();
        let triple = self.starts_with(&format!("{}{}", quote, quote));
        if triple { self.bump(); self.bump(); }
        let mut value = String::new();
        loop {
            let Some(ch) = self.peek() else { return Err(EyelingError::at("unterminated string literal", offset)); };
            self.bump();
            if ch == quote {
                if triple {
                    if self.starts_with(&format!("{}{}", quote, quote)) {
                        self.bump(); self.bump();
                        self.tokens.push(Token { kind: TokenKind::String(value), offset });
                        return Ok(());
                    }
                    value.push(ch);
                } else {
                    self.tokens.push(Token { kind: TokenKind::String(value), offset });
                    return Ok(());
                }
            } else if ch == '\\' {
                let Some(esc) = self.peek() else { return Err(EyelingError::at("unterminated string escape", offset)); };
                self.bump();
                match esc {
                    'n' => value.push('\n'),
                    'r' => value.push('\r'),
                    't' => value.push('\t'),
                    '"' => value.push('"'),
                    '\'' => value.push('\''),
                    '\\' => value.push('\\'),
                    other => value.push(other),
                }
            } else {
                value.push(ch);
            }
        }
    }

    fn read_number(&mut self) -> Result<()> {
        let offset = self.pos;
        let mut value = String::new();
        if self.peek() == Some('-') { value.push(self.bump().unwrap()); }
        while self.peek().is_some_and(|c| c.is_ascii_digit()) { value.push(self.bump().unwrap()); }
        if self.peek() == Some('.') && self.peek_next().is_some_and(|c| c.is_ascii_digit()) {
            value.push(self.bump().unwrap());
            while self.peek().is_some_and(|c| c.is_ascii_digit()) { value.push(self.bump().unwrap()); }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            value.push(self.bump().unwrap());
            if matches!(self.peek(), Some('+' | '-')) { value.push(self.bump().unwrap()); }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) { value.push(self.bump().unwrap()); }
        }
        self.tokens.push(Token { kind: TokenKind::Number(value), offset });
        Ok(())
    }

    fn read_word(&mut self) -> Result<()> {
        let offset = self.pos;
        let mut word = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() || matches!(ch, '{' | '}' | '[' | ']' | '(' | ')' | ',' | ';') { break; }
            if ch == '#' { break; }
            if ch == '.' { break; }
            if ch == '<' || ch == '>' || ch == '=' || ch == '^' { break; }
            word.push(ch);
            self.bump();
        }
        if word.is_empty() {
            return Err(EyelingError::at(format!("unexpected character {:?}", self.peek()), offset));
        }
        let kind = match word.as_str() {
            "@prefix" => TokenKind::AtPrefix,
            "@base" => TokenKind::AtBase,
            "PREFIX" | "prefix" => TokenKind::Prefix,
            "BASE" | "base" => TokenKind::Base,
            "a" => TokenKind::A,
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

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }
}
