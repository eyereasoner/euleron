use crate::ast::*;
use crate::error::{EyeronError, Result};
use crate::lexer::{lex, Token, TokenKind};

pub fn parse_n3(input: &str, base_iri: Option<&str>) -> Result<Document> {
    let tokens = lex(input)?;
    Parser::new(tokens, base_iri).parse_document()
}

/// Parse an RDF Message Log using the draft `VERSION "1.2-messages"` /
/// `MESSAGE` delimiter syntax and materialize Eyeron's internal replay view.
///
/// The replay view uses the `eymsg:` vocabulary: one stream resource, ordered
/// message envelopes, next-envelope links, payload kind, and one payload graph
/// resource per non-empty message.  Payload graph resources are connected to a
/// quoted formula with `log:nameOf`, so rules can inspect each message
/// atomically through `log:includes`.
pub fn parse_rdf_message_log(input: &str, base_iri: Option<&str>) -> Result<Document> {
    let (prefixes, messages) = split_rdf_message_log(input)?;
    let mut doc = Document::new();
    doc.base_iri = base_iri.map(ToOwned::to_owned);

    // Seed prefixes even if all messages are empty.
    if !prefixes.trim().is_empty() {
        let seed_text = format!("{}\n<urn:eyeron:prefix-seed> <urn:eyeron:prefix-seed> <urn:eyeron:prefix-seed> .\n", prefixes);
        let mut seed = parse_n3(&seed_text, base_iri)?;
        seed.facts.clear();
        doc.merge(seed);
    }

    let stream = Term::iri("urn:eyeron:rdf-message-stream:1");
    let envelope_terms: Vec<Term> = (0..messages.len())
        .map(|i| Term::iri(format!("urn:eyeron:rdf-message-stream:1:envelope:{}", i + 1)))
        .collect();

    let ey = |local: &str| Term::iri(format!("https://eyereasoner.github.io/eyeling/vocab/message#{}", local));
    let log = |local: &str| Term::iri(format!("http://www.w3.org/2000/10/swap/log#{}", local));

    doc.facts.push(Triple::new(stream.clone(), Term::iri(RDF_TYPE), ey("RDFMessageStream")));
    doc.facts.push(Triple::new(stream.clone(), ey("orderedEnvelopes"), Term::List(envelope_terms.clone())));
    if let Some(first) = envelope_terms.first() {
        doc.facts.push(Triple::new(stream.clone(), ey("firstEnvelope"), first.clone()));
    }

    for (idx, message) in messages.iter().enumerate() {
        let envelope = envelope_terms[idx].clone();
        doc.facts.push(Triple::new(stream.clone(), ey("envelope"), envelope.clone()));
        doc.facts.push(Triple::new(envelope.clone(), Term::iri(RDF_TYPE), ey("MessageEnvelope")));
        doc.facts.push(Triple::new(envelope.clone(), ey("offset"), number_literal(idx.to_string())));
        if idx + 1 < envelope_terms.len() {
            doc.facts.push(Triple::new(envelope.clone(), ey("nextEnvelope"), envelope_terms[idx + 1].clone()));
        }

        if message_is_empty(message) {
            doc.facts.push(Triple::new(envelope, ey("payloadKind"), ey("empty")));
            continue;
        }

        let rewritten = rewrite_message_blank_labels(message, idx + 1);
        let msg_text = format!("{}\n{}\n", prefixes, rewritten);
        let msg_doc = parse_n3(&msg_text, base_iri)?;
        for (k, v) in &msg_doc.prefixes {
            doc.prefixes.insert(k.clone(), v.clone());
        }

        let payload = Term::iri(format!("urn:eyeron:rdf-message-stream:1:payload:{}", idx + 1));
        doc.facts.push(Triple::new(envelope.clone(), ey("payloadKind"), ey("nonEmpty")));
        doc.facts.push(Triple::new(envelope, ey("payloadGraph"), payload.clone()));
        doc.facts.push(Triple::new(payload, log("nameOf"), Term::Formula(msg_doc.facts)));
    }

    Ok(doc)
}

pub fn is_rdf_message_log(input: &str) -> bool {
    input.lines().any(|line| line.trim_start().starts_with("VERSION \"1.2-messages\""))
        || input.lines().any(|line| line.trim() == "MESSAGE")
}

fn split_rdf_message_log(input: &str) -> Result<(String, Vec<String>)> {
    let mut prefixes = String::new();
    let mut current = String::new();
    let mut messages = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            current.push_str(line);
            current.push('\n');
            continue;
        }
        if trimmed.starts_with("VERSION ") {
            continue;
        }
        if trimmed == "MESSAGE" {
            messages.push(current.clone());
            current.clear();
            continue;
        }
        if trimmed.starts_with("PREFIX ") || trimmed.starts_with("prefix ") {
            prefixes.push_str(&normalize_turtle_directive(trimmed, "PREFIX", "@prefix")?);
            prefixes.push('\n');
            continue;
        }
        if trimmed.starts_with("BASE ") || trimmed.starts_with("base ") {
            prefixes.push_str(&normalize_turtle_directive(trimmed, "BASE", "@base")?);
            prefixes.push('\n');
            continue;
        }
        current.push_str(line);
        current.push('\n');
    }
    messages.push(current);
    Ok((prefixes, messages))
}

fn normalize_turtle_directive(line: &str, upper: &str, n3: &str) -> Result<String> {
    let rest = if line.len() >= upper.len() && line[..upper.len()].eq_ignore_ascii_case(upper) {
        line[upper.len()..].trim()
    } else {
        return Err(EyeronError::new(format!("expected {} directive", upper)));
    };
    let without_dot = rest.strip_suffix('.').unwrap_or(rest).trim();
    Ok(format!("{} {} .", n3, without_dot))
}

fn message_is_empty(message: &str) -> bool {
    message.lines().all(|line| {
        let trimmed = line.trim();
        trimmed.is_empty() || trimmed.starts_with('#')
    })
}

fn rewrite_message_blank_labels(message: &str, message_index: usize) -> String {
    let mut out = String::with_capacity(message.len() + 16);
    let mut chars = message.char_indices().peekable();
    let mut in_string: Option<char> = None;
    let mut escaped = false;

    while let Some((_idx, ch)) = chars.next() {
        if let Some(quote) = in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = None;
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            in_string = Some(ch);
            out.push(ch);
            continue;
        }

        if ch == '#' {
            out.push(ch);
            while let Some((_j, c)) = chars.next() {
                out.push(c);
                if c == '\n' { break; }
            }
            continue;
        }

        if ch == '_' {
            if let Some(&(_, ':')) = chars.peek() {
                chars.next(); // consume ':'
                let mut label = String::new();
                while let Some(&(_, c)) = chars.peek() {
                    if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                        label.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if !label.is_empty() {
                    out.push_str(&format!("_:m{}_{}", message_index, label));
                    continue;
                }
                out.push('_');
                out.push(':');
                continue;
            }
        }

        out.push(ch);
    }

    out
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
        // The notation3tests static corpus uses the standard N3/RDF prefixes
        // in a few files without redeclaring every one of them.  Seed the
        // conventional prefixes while still letting explicit declarations
        // override them.
        doc.prefixes.insert("rdf".to_string(), "http://www.w3.org/1999/02/22-rdf-syntax-ns#".to_string());
        doc.prefixes.insert("rdfs".to_string(), "http://www.w3.org/2000/01/rdf-schema#".to_string());
        doc.prefixes.insert("xsd".to_string(), "http://www.w3.org/2001/XMLSchema#".to_string());
        doc.prefixes.insert("log".to_string(), "http://www.w3.org/2000/10/swap/log#".to_string());
        doc.prefixes.insert("list".to_string(), "http://www.w3.org/2000/10/swap/list#".to_string());
        doc.prefixes.insert("math".to_string(), "http://www.w3.org/2000/10/swap/math#".to_string());
        doc.prefixes.insert("string".to_string(), "http://www.w3.org/2000/10/swap/string#".to_string());
        doc.prefixes.insert("time".to_string(), "http://www.w3.org/2000/10/swap/time#".to_string());
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
            _ => return Err(EyeronError::at("expected prefix name", t.offset)),
        };
        let iri_tok = self.advance().clone();
        let iri = match iri_tok.kind {
            TokenKind::Iri(i) => self.resolve_iri(&i),
            _ => return Err(EyeronError::at("expected prefix IRI", iri_tok.offset)),
        };
        self.doc.prefixes.insert(name, iri);
        // Turtle-style PREFIX inside formulas does not require a trailing dot.
        // @prefix still normally has one; accepting it optionally here keeps the
        // parser permissive for the conformance corpus.
        if self.check(&TokenKind::Dot) { self.advance(); }
        Ok(())
    }

    fn parse_base(&mut self) -> Result<()> {
        self.advance();
        let iri_tok = self.advance().clone();
        let iri = match iri_tok.kind {
            TokenKind::Iri(i) => self.resolve_iri(&i),
            _ => return Err(EyeronError::at("expected base IRI", iri_tok.offset)),
        };
        self.doc.base_iri = Some(iri);
        if self.check(&TokenKind::Dot) { self.advance(); }
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
            _ => {
                // `true false true .` is a valid N3 triple with boolean terms.
                let facts = self.parse_triples_sequence_from_subject(boolean_literal(true))?;
                self.doc.facts.extend(facts);
            }
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
                        // Otherwise the leading formula is an ordinary term subject, e.g.
                        // `{ :A :B :C } a :Statement .` or a formula-valued predicate.
                        let subject = Term::formula(lhs);
                        let mut triples = Vec::new();
                        loop {
                            loop {
                                let (object, mut generated) = self.parse_term()?;
                                triples.push(Triple::new(subject.clone(), other.clone(), object));
                                triples.append(&mut generated);
                                if self.check(&TokenKind::Comma) { self.advance(); continue; }
                                break;
                            }
                            if self.check(&TokenKind::Semicolon) {
                                while self.check(&TokenKind::Semicolon) { self.advance(); }
                                if matches!(self.peek_kind(), TokenKind::Dot | TokenKind::RBrace | TokenKind::RBracket) { break; }
                                let next_pred = self.parse_verb()?;
                                let mut more = Vec::new();
                                loop {
                                    let (object, mut generated) = self.parse_term()?;
                                    more.push(Triple::new(subject.clone(), next_pred.clone(), object));
                                    more.append(&mut generated);
                                    if self.check(&TokenKind::Comma) { self.advance(); continue; }
                                    break;
                                }
                                triples.extend(more);
                                break;
                            }
                            break;
                        }
                        self.doc.facts.extend(triples);
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
        if matches!(self.peek_kind(), TokenKind::Boolean(false)) {
            return Err(EyeronError::at("false rule conclusions are not supported", self.peek().offset));
        }
        self.parse_formula()
    }

    fn parse_forward_rule_rhs(&mut self) -> Result<Vec<Triple>> {
        if matches!(self.peek_kind(), TokenKind::Boolean(true)) {
            self.advance();
            return Ok(Vec::new());
        }
        if matches!(self.peek_kind(), TokenKind::Boolean(false)) {
            return Err(EyeronError::at("false rule conclusions are not supported", self.peek().offset));
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
            return Err(EyeronError::new("generated triples cannot appear around an unquoted RHS term"));
        }
        Ok(vec![Triple::new(Term::iri(EYERON_UNQUOTE), Term::iri(EYERON_UNQUOTE), term)])
    }

    fn parse_formula(&mut self) -> Result<Vec<Triple>> {
        self.expect(TokenKind::LBrace)?;
        let mut triples = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Dot) { self.advance(); continue; }
            if matches!(self.peek_kind(), TokenKind::AtPrefix | TokenKind::Prefix) {
                self.parse_prefix()?;
                continue;
            }
            if matches!(self.peek_kind(), TokenKind::AtBase | TokenKind::Base) {
                self.parse_base()?;
                continue;
            }
            triples.extend(self.parse_triples_sequence()?);
            if self.check(&TokenKind::Dot) { self.advance(); }
            else if !self.check(&TokenKind::RBrace) {
                return Err(EyeronError::at("expected '.' or '}' in formula", self.peek().offset));
            }
        }
        self.expect(TokenKind::RBrace)?;
        Ok(triples)
    }

    fn parse_triples_sequence_from_subject(&mut self, subject: Term) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();
        triples.extend(self.parse_predicate_object_list(subject)?);
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
            if is_boolean_false_term(&object) {
                return Err(EyeronError::at("false rule conclusions are not supported", self.peek().offset));
            }
            if matches!((&subject, &object), (Term::Formula(_), Term::Var(_))) {
                return Err(EyeronError::at("formula-to-variable implication is not supported", self.peek().offset));
            }
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

            // N3 reverse-property path: `S <- :p O` means `O :p S`.
            if self.check(&TokenKind::Reverse) {
                self.advance();
                let predicate = self.parse_verb()?;
                loop {
                    let (object, mut generated) = self.parse_term()?;
                    triples.push(Triple::new(object, predicate.clone(), subject.clone()));
                    triples.append(&mut generated);
                    if self.check(&TokenKind::Comma) { self.advance(); continue; }
                    break;
                }
            }
            // N3 alternate spelling: `S is :p of O` means `O :p S`.
            else if self.peek_is_bare("is") {
                self.advance();
                let predicate = self.parse_verb()?;
                if !self.peek_is_bare("of") {
                    return Err(EyeronError::at("expected 'of' after 'is <predicate>'", self.peek().offset));
                }
                self.advance();
                loop {
                    let (object, mut generated) = self.parse_term()?;
                    triples.push(Triple::new(object, predicate.clone(), subject.clone()));
                    triples.append(&mut generated);
                    if self.check(&TokenKind::Comma) { self.advance(); continue; }
                    break;
                }
            }
            // N3 alternate spelling: `S has :p O` means `S :p O`.
            else {
                if self.peek_is_bare("has") { self.advance(); }
                let predicate = self.parse_verb()?;
                loop {
                    let (object, mut generated) = self.parse_term()?;
                    triples.push(Triple::new(subject.clone(), predicate.clone(), object));
                    triples.append(&mut generated);
                    if self.check(&TokenKind::Comma) { self.advance(); continue; }
                    break;
                }
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
        let (term, _generated) = self.parse_term()?;
        // Predicate terms may themselves be lists or formulas in N3.  Generated
        // list-support triples are not needed to match such predicate terms, so
        // keep the predicate and drop the auxiliary triples here.
        Ok(term)
    }

    fn parse_term(&mut self) -> Result<(Term, Vec<Triple>)> {
        let tok = self.advance().clone();
        match tok.kind {
            TokenKind::Iri(i) => Ok((Term::iri(self.resolve_iri(&i)), Vec::new())),
            TokenKind::PName(p) => self.parse_pname_or_path(&p, tok.offset),
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
            other => Err(EyeronError::at(format!("expected term, got {:?}", other), tok.offset)),
        }
    }

    fn finish_literal(&mut self, value: String) -> Result<(Term, Vec<Triple>)> {
        let mut lit = Literal::plain(value);
        if self.check(&TokenKind::HatHat) {
            self.advance();
            let (dt, generated) = self.parse_term()?;
            if !generated.is_empty() { return Err(EyeronError::new("datatype cannot generate triples")); }
            match dt {
                Term::Iri(i) => lit.datatype = Some(i),
                _ => return Err(EyeronError::new("datatype must be an IRI")),
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
        // N3's `id` form lets a property list name its subject:
        // `[ id :c :d :e ]` is parsed as triples about `:c`, not about a
        // fresh blank node.  This is used in the notation3 static grammar
        // tests and is a compact way to nest property lists.
        if self.peek_is_bare("id") {
            self.advance();
            let (subject, mut generated) = self.parse_term()?;
            let mut triples = Vec::new();
            triples.append(&mut generated);
            triples.extend(self.parse_predicate_object_list(subject.clone())?);
            self.expect(TokenKind::RBracket)?;
            return Ok((subject, triples));
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
        let list_term = Term::list(items.clone());
        if !items.is_empty() {
            triples.push(Triple::new(list_term.clone(), Term::iri(RDF_FIRST), items[0].clone()));
            let rest = if items.len() == 1 { Term::iri(RDF_NIL) } else { Term::list(items[1..].to_vec()) };
            triples.push(Triple::new(list_term.clone(), Term::iri(RDF_REST), rest));
        }
        Ok((list_term, triples))
    }

    fn parse_pname_or_path(&mut self, pname: &str, offset: usize) -> Result<(Term, Vec<Triple>)> {
        if pname.contains('!') || pname.contains('^') {
            return self.parse_path_pname(pname, offset);
        }
        Ok((Term::iri(self.expand_pname(pname, offset)?), Vec::new()))
    }

    fn parse_path_pname(&mut self, pname: &str, offset: usize) -> Result<(Term, Vec<Triple>)> {
        let mut parts = Vec::<(char, String)>::new();
        let mut buf = String::new();
        let mut op = '!';
        for ch in pname.chars() {
            if ch == '!' || ch == '^' {
                if buf.is_empty() { return Err(EyeronError::at("empty path component", offset)); }
                parts.push((op, buf.clone()));
                buf.clear();
                op = ch;
            } else {
                buf.push(ch);
            }
        }
        if buf.is_empty() { return Err(EyeronError::at("empty path component", offset)); }
        parts.push((op, buf));
        let mut current = Term::iri(self.expand_pname(&parts[0].1, offset)?);
        let mut generated = Vec::new();
        for (op, raw_pred) in parts.into_iter().skip(1) {
            let pred = Term::iri(self.expand_pname(&raw_pred, offset)?);
            let next = self.fresh_blank("path");
            if op == '!' {
                generated.push(Triple::new(current, pred, next.clone()));
            } else {
                generated.push(Triple::new(next.clone(), pred, current));
            }
            current = next;
        }
        Ok((current, generated))
    }

    fn expand_pname(&self, pname: &str, offset: usize) -> Result<String> {
        if let Some((prefix, local)) = pname.split_once(':') {
            let Some(base) = self.doc.prefixes.get(prefix) else {
                return Err(EyeronError::at(format!("unknown prefix '{}:'", prefix), offset));
            };
            return Ok(format!("{}{}", base, local));
        }
        if let Some(base) = self.doc.base_iri.as_deref() {
            return Ok(self.resolve_iri_against_base(base, pname));
        }
        if let Some(base) = self.doc.prefixes.get("") {
            return Ok(format!("{}{}", base, pname));
        }
        Ok(format!("http://example.org/{}", pname))
    }

    fn resolve_iri(&self, iri: &str) -> String {
        if has_uri_scheme(iri) { return iri.to_string(); }
        let Some(base) = &self.doc.base_iri else { return iri.to_string(); };
        self.resolve_iri_against_base(base, iri)
    }

    fn resolve_iri_against_base(&self, base: &str, reference: &str) -> String {
        if has_uri_scheme(reference) { return reference.to_string(); }
        let (base_scheme, base_authority, base_path, base_query, _) = split_uri(base);
        let (ref_path, ref_query, ref_fragment) = split_reference(reference);

        let (authority, path, query) = if let Some(rest) = ref_path.strip_prefix("//") {
            let (auth, p) = split_authority_path(rest);
            (Some(auth), remove_dot_segments(&p), ref_query)
        } else if ref_path.is_empty() {
            (base_authority.clone(), base_path.clone(), ref_query.or(base_query))
        } else if ref_path.starts_with('/') {
            (base_authority.clone(), remove_dot_segments(&ref_path), ref_query)
        } else {
            let merged = merge_paths(&base_path, &ref_path, base_authority.is_some());
            (base_authority.clone(), remove_dot_segments(&merged), ref_query)
        };

        let mut out = String::new();
        if let Some(scheme) = base_scheme { out.push_str(&scheme); out.push(':'); }
        if let Some(auth) = authority { out.push_str("//"); out.push_str(&auth); }
        out.push_str(&path);
        if let Some(q) = query { out.push('?'); out.push_str(&q); }
        if let Some(f) = ref_fragment { out.push('#'); out.push_str(&f); }
        out
    }

    fn peek_is_bare(&self, word: &str) -> bool {
        matches!(self.peek_kind(), TokenKind::PName(p) if p.eq_ignore_ascii_case(word))
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
            Err(EyeronError::at(format!("expected {:?}, got {:?}", expected, self.peek_kind()), self.peek().offset))
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

fn number_literal(mut value: String) -> Term {
    if value.starts_with('+') { value.remove(0); }
    let datatype = if value.contains('e') || value.contains('E') {
        let parsed = value.parse::<f64>().unwrap_or(0.0);
        value = trim_numeric_lexical(parsed, true);
        "http://www.w3.org/2001/XMLSchema#double"
    } else if value.contains('.') {
        if value.starts_with('.') { value.insert(0, '0'); }
        if value.starts_with("-.") { value.insert(1, '0'); }
        if value.ends_with('.') { value.push('0'); }
        "http://www.w3.org/2001/XMLSchema#decimal"
    } else {
        "http://www.w3.org/2001/XMLSchema#integer"
    };
    Term::Literal(Literal { value, datatype: Some(datatype.to_string()), language: None })
}

fn trim_numeric_lexical(value: f64, decimal: bool) -> String {
    if value.is_nan() { return "NaN".to_string(); }
    if value.is_infinite() { return if value.is_sign_negative() { "-INF" } else { "INF" }.to_string(); }
    let mut s = value.to_string();
    if s.contains('.') {
        while s.ends_with('0') { s.pop(); }
        if s.ends_with('.') { s.push('0'); }
    } else if decimal {
        s.push_str(".0");
    }
    s
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

fn is_boolean_false_term(term: &Term) -> bool {
    match term {
        Term::Literal(lit) => {
            lit.value == "false"
                && lit.language.is_none()
                && lit.datatype.as_deref() == Some("http://www.w3.org/2001/XMLSchema#boolean")
        }
        _ => false,
    }
}


fn has_uri_scheme(s: &str) -> bool {
    let Some(idx) = s.find(':') else { return false; };
    let prefix = &s[..idx];
    !prefix.is_empty()
        && prefix.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && prefix.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn split_uri(uri: &str) -> (Option<String>, Option<String>, String, Option<String>, Option<String>) {
    let (scheme, rest) = if let Some(idx) = uri.find(':') {
        (Some(uri[..idx].to_string()), &uri[idx + 1..])
    } else {
        (None, uri)
    };
    let (authority, rest) = if let Some(after) = rest.strip_prefix("//") {
        let (auth, path) = split_authority_path(after);
        (Some(auth), path)
    } else {
        (None, rest.to_string())
    };
    let (path_q, fragment) = match rest.split_once('#') { Some((a, b)) => (a, Some(b.to_string())), None => (rest.as_str(), None) };
    let (path, query) = match path_q.split_once('?') { Some((a, b)) => (a.to_string(), Some(b.to_string())), None => (path_q.to_string(), None) };
    (scheme, authority, path, query, fragment)
}

fn split_reference(reference: &str) -> (String, Option<String>, Option<String>) {
    let (path_q, fragment) = match reference.split_once('#') { Some((a, b)) => (a, Some(b.to_string())), None => (reference, None) };
    let (path, query) = match path_q.split_once('?') { Some((a, b)) => (a.to_string(), Some(b.to_string())), None => (path_q.to_string(), None) };
    (path, query, fragment)
}

fn split_authority_path(s: &str) -> (String, String) {
    if let Some(idx) = s.find('/') { (s[..idx].to_string(), s[idx..].to_string()) } else { (s.to_string(), String::new()) }
}

fn merge_paths(base_path: &str, rel_path: &str, has_authority: bool) -> String {
    if has_authority && base_path.is_empty() { return format!("/{}", rel_path); }
    match base_path.rfind('/') {
        Some(idx) => format!("{}{}", &base_path[..idx + 1], rel_path),
        None => rel_path.to_string(),
    }
}

fn remove_dot_segments(path: &str) -> String {
    let leading = path.starts_with('/');
    let trailing = path.ends_with('/') || path.ends_with("/.") || path.ends_with("/..");
    let mut stack: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => { stack.pop(); }
            other => stack.push(other),
        }
    }
    let mut out = String::new();
    if leading { out.push('/'); }
    out.push_str(&stack.join("/"));
    if trailing && !out.ends_with('/') { out.push('/'); }
    if out.is_empty() && leading { out.push('/'); }
    out
}
