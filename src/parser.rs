use crate::ast::*;
use crate::error::{EyelingError, Result};
use crate::lexer::{lex, Token, TokenKind};

pub fn parse_n3(input: &str, base_iri: Option<&str>) -> Result<Document> {
    let tokens = lex(input)?;
    Parser::new(tokens, base_iri).parse_document()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    doc: Document,
    blank_counter: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>, base_iri: Option<&str>) -> Self {
        let mut doc = Document::new();
        doc.base_iri = base_iri.map(ToOwned::to_owned);
        Self { tokens, pos: 0, doc, blank_counter: 0 }
    }

    fn parse_document(mut self) -> Result<Document> {
        while !self.check(&TokenKind::Eof) {
            match self.peek_kind() {
                TokenKind::AtPrefix | TokenKind::Prefix => self.parse_prefix()?,
                TokenKind::AtBase | TokenKind::Base => self.parse_base()?,
                TokenKind::LBrace => self.parse_formula_statement()?,
                TokenKind::Boolean(true) => self.parse_true_formula_statement()?,
                TokenKind::Dot => { self.advance(); }
                _ => {
                    let facts = self.parse_triples_sequence()?;
                    self.doc.facts.extend(facts);
                    self.expect_dot()?;
                }
            }
        }
        Ok(self.doc)
    }

    fn parse_prefix(&mut self) -> Result<()> {
        self.advance();
        let t = self.advance().clone();
        let name = match t.kind {
            TokenKind::PName(p) => p.strip_suffix(':').unwrap_or(&p).to_string(),
            _ => return Err(EyelingError::at("expected prefix name", t.offset)),
        };
        let iri_tok = self.advance().clone();
        let iri = match iri_tok.kind {
            TokenKind::Iri(i) => i,
            _ => return Err(EyelingError::at("expected prefix IRI", iri_tok.offset)),
        };
        self.doc.prefixes.insert(name, iri);
        self.expect_dot()?;
        Ok(())
    }

    fn parse_base(&mut self) -> Result<()> {
        self.advance();
        let iri_tok = self.advance().clone();
        let iri = match iri_tok.kind {
            TokenKind::Iri(i) => i,
            _ => return Err(EyelingError::at("expected base IRI", iri_tok.offset)),
        };
        self.doc.base_iri = Some(iri);
        self.expect_dot()?;
        Ok(())
    }

    fn parse_true_formula_statement(&mut self) -> Result<()> {
        self.advance();
        match self.peek_kind() {
            TokenKind::Arrow => {
                self.advance();
                let rhs = self.parse_forward_rule_rhs()?;
                self.doc.rules.push(Rule { premise: Vec::new(), conclusion: rhs, is_forward: true });
            }
            TokenKind::BackArrow => {
                self.advance();
                let rhs = self.parse_formula_or_true()?;
                self.doc.rules.push(Rule { premise: rhs, conclusion: Vec::new(), is_forward: false });
            }
            _ => return Err(EyelingError::at("expected => or <= after true", self.peek().offset)),
        }
        self.expect_dot()?;
        Ok(())
    }

    fn parse_formula_statement(&mut self) -> Result<()> {
        let lhs = self.parse_formula()?;
        match self.peek_kind() {
            TokenKind::Arrow => {
                self.advance();
                let rhs = self.parse_forward_rule_rhs()?;
                self.doc.rules.push(Rule { premise: lhs, conclusion: rhs, is_forward: true });
            }
            TokenKind::BackArrow => {
                self.advance();
                let rhs = self.parse_formula_or_true()?;
                // `{ head } <= { body }` is a backward rule: use it
                // goal-directed when a forward premise asks for `head`.
                self.doc.rules.push(Rule { premise: rhs, conclusion: lhs, is_forward: false });
            }
            _ => {
                let predicate = self.parse_verb()?;
                match predicate {
                    Term::Iri(ref iri) if iri == LOG_QUERY => {
                        let rhs = self.parse_formula_or_true()?;
                        self.doc.rules.push(Rule { premise: lhs, conclusion: rhs, is_forward: true });
                    }
                    other => {
                        return Err(EyelingError::at(
                            format!("expected =>, <=, or log:query after formula, got {:?}", other),
                            self.peek().offset,
                        ));
                    }
                }
            }
        }
        self.expect_dot()?;
        Ok(())
    }


    fn parse_formula_or_true(&mut self) -> Result<Vec<Triple>> {
        if matches!(self.peek_kind(), TokenKind::Boolean(true)) {
            self.advance();
            return Ok(Vec::new());
        }
        self.parse_formula()
    }

    fn parse_forward_rule_rhs(&mut self) -> Result<Vec<Triple>> {
        if matches!(self.peek_kind(), TokenKind::Boolean(true)) {
            self.advance();
            return Ok(Vec::new());
        }
        if matches!(self.peek_kind(), TokenKind::LBrace) {
            return self.parse_formula();
        }

        // N3 allows a forward-rule RHS to be a term that resolves to a quoted
        // formula, e.g. `{ :a :b ?F } => ?F .`.  Represent that as an internal
        // unquote instruction; the reasoner expands the formula contents when
        // the rule fires.
        let (term, generated) = self.parse_term()?;
        if !generated.is_empty() {
            return Err(EyelingError::new("generated triples cannot appear around an unquoted RHS term"));
        }
        Ok(vec![Triple::new(Term::iri(EYELING_UNQUOTE), Term::iri(EYELING_UNQUOTE), term)])
    }

    fn parse_formula(&mut self) -> Result<Vec<Triple>> {
        self.expect(TokenKind::LBrace)?;
        let mut triples = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Dot) { self.advance(); continue; }
            triples.extend(self.parse_triples_sequence()?);
            if self.check(&TokenKind::Dot) { self.advance(); }
            else if !self.check(&TokenKind::RBrace) {
                return Err(EyelingError::at("expected '.' or '}' in formula", self.peek().offset));
            }
        }
        self.expect(TokenKind::RBrace)?;
        Ok(triples)
    }

    fn parse_triples_sequence(&mut self) -> Result<Vec<Triple>> {
        let (subject, mut generated) = self.parse_term()?;
        let mut triples = Vec::new();
        triples.append(&mut generated);

        // N3 implication can appear as a statement inside a quoted formula,
        // for example `{ { ?x a :Cat } => { ?x a :Animal } . }`.
        // Store it as a first-class triple whose subject/object are formula terms;
        // the reasoner promotes derived implication triples to active rules.
        if self.check(&TokenKind::Arrow) || self.check(&TokenKind::BackArrow) {
            let backward = self.check(&TokenKind::BackArrow);
            self.advance();
            let (object, mut object_generated) = self.parse_term()?;
            let object = if is_boolean_true_term(&object) { Term::Formula(Vec::new()) } else { object };
            triples.append(&mut object_generated);
            if backward {
                // Preserve `<=` polarity in quoted rule terms.  These are
                // promoted as backward rules and printed again as `<=`.
                triples.push(Triple::new(subject, Term::iri(LOG_IMPLIED_BY), object));
            } else {
                triples.push(Triple::new(subject, Term::iri(LOG_IMPLIES), object));
            }
            return Ok(triples);
        }

        triples.extend(self.parse_predicate_object_list(subject)?);
        Ok(triples)
    }

    fn parse_predicate_object_list(&mut self, subject: Term) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();
        loop {
            if matches!(self.peek_kind(), TokenKind::Dot | TokenKind::RBrace | TokenKind::RBracket) { break; }
            let predicate = self.parse_verb()?;
            loop {
                let (object, mut generated) = self.parse_term()?;
                triples.push(Triple::new(subject.clone(), predicate.clone(), object));
                triples.append(&mut generated);
                if self.check(&TokenKind::Comma) { self.advance(); continue; }
                break;
            }
            if self.check(&TokenKind::Semicolon) {
                while self.check(&TokenKind::Semicolon) { self.advance(); }
                if matches!(self.peek_kind(), TokenKind::Dot | TokenKind::RBrace | TokenKind::RBracket) { break; }
                continue;
            }
            break;
        }
        Ok(triples)
    }

    fn parse_verb(&mut self) -> Result<Term> {
        if self.check(&TokenKind::A) {
            self.advance();
            return Ok(Term::iri(RDF_TYPE));
        }
        if self.check(&TokenKind::Equals) {
            self.advance();
            return Ok(Term::iri(OWL_SAME_AS));
        }
        if self.check(&TokenKind::Arrow) {
            self.advance();
            return Ok(Term::iri(LOG_IMPLIES));
        }
        if self.check(&TokenKind::BackArrow) {
            self.advance();
            return Ok(Term::iri(LOG_IMPLIED_BY));
        }
        let (term, generated) = self.parse_term()?;
        if !generated.is_empty() {
            return Err(EyelingError::new("generated blank/list triples cannot be used as predicates"));
        }
        Ok(term)
    }

    fn parse_term(&mut self) -> Result<(Term, Vec<Triple>)> {
        let tok = self.advance().clone();
        match tok.kind {
            TokenKind::Iri(i) => Ok((Term::iri(self.resolve_iri(&i)), Vec::new())),
            TokenKind::PName(p) => Ok((Term::iri(self.expand_pname(&p, tok.offset)?), Vec::new())),
            TokenKind::Var(v) => Ok((Term::var(v), Vec::new())),
            TokenKind::Blank(b) => Ok((Term::blank(b), Vec::new())),
            TokenKind::String(value) => self.finish_literal(value),
            TokenKind::Number(value) => Ok((number_literal(value), Vec::new())),
            TokenKind::Boolean(value) => Ok((boolean_literal(value), Vec::new())),
            TokenKind::A => Ok((Term::iri(RDF_TYPE), Vec::new())),
            TokenKind::LBrace => {
                self.pos -= 1;
                let triples = self.parse_formula()?;
                Ok((Term::formula(triples), Vec::new()))
            }
            TokenKind::LBracket => self.parse_blank_node_property_list(),
            TokenKind::LParen => self.parse_list(),
            other => Err(EyelingError::at(format!("expected term, got {:?}", other), tok.offset)),
        }
    }

    fn finish_literal(&mut self, value: String) -> Result<(Term, Vec<Triple>)> {
        let mut lit = Literal::plain(value);
        if self.check(&TokenKind::HatHat) {
            self.advance();
            let (dt, generated) = self.parse_term()?;
            if !generated.is_empty() { return Err(EyelingError::new("datatype cannot generate triples")); }
            match dt {
                Term::Iri(i) => lit.datatype = Some(i),
                _ => return Err(EyelingError::new("datatype must be an IRI")),
            }
        } else if let TokenKind::Lang(lang) = self.peek_kind() {
            lit.language = Some(lang.clone());
            self.advance();
        }
        Ok((Term::Literal(lit), Vec::new()))
    }

    fn parse_blank_node_property_list(&mut self) -> Result<(Term, Vec<Triple>)> {
        let blank = self.fresh_blank("b");
        if self.check(&TokenKind::RBracket) {
            self.advance();
            return Ok((blank, Vec::new()));
        }
        let triples = self.parse_predicate_object_list(blank.clone())?;
        self.expect(TokenKind::RBracket)?;
        Ok((blank, triples))
    }

    fn parse_list(&mut self) -> Result<(Term, Vec<Triple>)> {
        let mut items = Vec::new();
        let mut triples = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
            let (item, mut generated) = self.parse_term()?;
            items.push(item);
            triples.append(&mut generated);
        }
        self.expect(TokenKind::RParen)?;
        Ok((Term::list(items), triples))
    }

    fn expand_pname(&self, pname: &str, offset: usize) -> Result<String> {
        let Some((prefix, local)) = pname.split_once(':') else {
            return Err(EyelingError::at(format!("unknown bare name '{}'; use a prefix or <IRI>", pname), offset));
        };
        let Some(base) = self.doc.prefixes.get(prefix) else {
            return Err(EyelingError::at(format!("unknown prefix '{}:'", prefix), offset));
        };
        Ok(format!("{}{}", base, local))
    }

    fn resolve_iri(&self, iri: &str) -> String {
        if iri.contains("://") || iri.starts_with("urn:") || iri.starts_with("mailto:") { return iri.to_string(); }
        let Some(base) = &self.doc.base_iri else { return iri.to_string(); };
        if iri.starts_with('#') { return format!("{}{}", base, iri); }
        if base.ends_with('/') || base.ends_with('#') { return format!("{}{}", base, iri); }
        match base.rfind('/') {
            Some(idx) => format!("{}{}", &base[..idx + 1], iri),
            None => format!("{}{}", base, iri),
        }
    }

    fn fresh_blank(&mut self, prefix: &str) -> Term {
        self.blank_counter += 1;
        Term::blank(format!("{}{}", prefix, self.blank_counter))
    }

    fn expect_dot(&mut self) -> Result<()> { self.expect(TokenKind::Dot) }

    fn expect(&mut self, expected: TokenKind) -> Result<()> {
        if self.check(&expected) {
            self.advance();
            Ok(())
        } else {
            Err(EyelingError::at(format!("expected {:?}, got {:?}", expected, self.peek_kind()), self.peek().offset))
        }
    }

    fn check(&self, expected: &TokenKind) -> bool { same_variant(self.peek_kind(), expected) }

    fn advance(&mut self) -> &Token {
        if self.pos < self.tokens.len().saturating_sub(1) { self.pos += 1; }
        &self.tokens[self.pos - 1]
    }

    fn peek(&self) -> &Token { &self.tokens[self.pos] }

    fn peek_kind(&self) -> &TokenKind { &self.peek().kind }
}

fn same_variant(a: &TokenKind, b: &TokenKind) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

fn number_literal(value: String) -> Term {
    let datatype = if value.contains('.') || value.contains('e') || value.contains('E') {
        "http://www.w3.org/2001/XMLSchema#decimal"
    } else {
        "http://www.w3.org/2001/XMLSchema#integer"
    };
    Term::Literal(Literal { value, datatype: Some(datatype.to_string()), language: None })
}

fn boolean_literal(value: bool) -> Term {
    Term::Literal(Literal {
        value: if value { "true" } else { "false" }.to_string(),
        datatype: Some("http://www.w3.org/2001/XMLSchema#boolean".to_string()),
        language: None,
    })
}

fn is_boolean_true_term(term: &Term) -> bool {
    match term {
        Term::Literal(lit) => {
            lit.value == "true"
                && lit.language.is_none()
                && lit.datatype.as_deref() == Some("http://www.w3.org/2001/XMLSchema#boolean")
        }
        _ => false,
    }
}
