use crate::ast::*;
use crate::error::{EyeronError, Result};
use crate::lexer::{lex, Token, TokenKind};
use crate::rdf_compat::RdfFormat;

pub fn parse_n3(input: &str, base_iri: Option<&str>) -> Result<Document> {
    parse_n3_with_source(input, base_iri, None)
}

pub fn parse_n3_with_source(input: &str, base_iri: Option<&str>, source_label: Option<&str>) -> Result<Document> {
    let tokens = lex(input)?;
    let line_starts = line_starts(input);
    Parser::new(tokens, base_iri).with_source(source_label, line_starts).parse_document()
}

pub(crate) fn parse_rdf12_compat(input: &str, base_iri: Option<&str>, format: RdfFormat) -> Result<Document> {
    let tokens = lex(input)?;
    Parser::with_profile(tokens, base_iri, ParserProfile::rdf12(format)).parse_document()
}

fn line_starts(input: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (idx, ch) in input.char_indices() {
        if ch == '\n' { starts.push(idx + ch.len_utf8()); }
    }
    starts
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

fn named_graph_formula(graph: Term, triples: Vec<Triple>) -> Triple {
    Triple::new(graph, Term::iri(LOG_NAME_OF), Term::Formula(triples))
}

const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";

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
            for (_j, c) in chars.by_ref() {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParserProfile {
    syntax: SyntaxProfile,
    emit_native_list_triples: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyntaxProfile {
    N3,
    Rdf12(RdfFormat),
}

impl ParserProfile {
    fn n3() -> Self { Self { syntax: SyntaxProfile::N3, emit_native_list_triples: true } }

    fn rdf12(format: RdfFormat) -> Self {
        Self { syntax: SyntaxProfile::Rdf12(format), emit_native_list_triples: false }
    }

    fn rdf_format(&self) -> Option<RdfFormat> {
        match self.syntax {
            SyntaxProfile::N3 => None,
            SyntaxProfile::Rdf12(format) => Some(format),
        }
    }

    fn is_rdf12(&self) -> bool { self.rdf_format().is_some() }

    fn is_line_syntax(&self) -> bool {
        matches!(self.rdf_format(), Some(RdfFormat::NTriples | RdfFormat::NQuads))
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    doc: Document,
    blank_counter: usize,
    profile: ParserProfile,
    source_label: Option<String>,
    line_starts: Vec<usize>,
}

impl Parser {
    fn new(tokens: Vec<Token>, base_iri: Option<&str>) -> Self {
        Self::with_profile(tokens, base_iri, ParserProfile::n3())
    }

    fn with_profile(tokens: Vec<Token>, base_iri: Option<&str>, profile: ParserProfile) -> Self {
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
        Self { tokens, pos: 0, doc, blank_counter: 0, profile, source_label: None, line_starts: Vec::new() }
    }

    fn with_source(mut self, source_label: Option<&str>, line_starts: Vec<usize>) -> Self {
        self.source_label = source_label.map(ToOwned::to_owned);
        self.line_starts = line_starts;
        self
    }

    fn source_ref_at(&self, offset: usize) -> Option<SourceRef> {
        let label = self.source_label.as_ref()?;
        let line = match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx + 1,
            Err(idx) => idx,
        };
        Some(SourceRef { label: label.clone(), line: line.max(1) })
    }

    fn add_fact(&mut self, fact: Triple, source: Option<SourceRef>) {
        if let Some(source) = source {
            self.doc.fact_sources.entry(fact.clone()).or_insert(source);
        }
        self.doc.facts.push(fact);
    }

    fn add_facts<I>(&mut self, facts: I, source: Option<SourceRef>)
    where
        I: IntoIterator<Item = Triple>,
    {
        for fact in facts {
            self.add_fact(fact, source.clone());
        }
    }

    fn parse_document(mut self) -> Result<Document> {
        if self.profile.is_line_syntax() {
            self.parse_rdf_line_document()?;
            return Ok(self.doc);
        }

        while !self.check(&TokenKind::Eof) {
            if self.profile.is_rdf12() {
                self.parse_rdf12_statement()?;
                continue;
            }
            match self.peek_kind() {
                TokenKind::AtPrefix | TokenKind::Prefix => self.parse_prefix()?,
                TokenKind::AtBase | TokenKind::Base => self.parse_base()?,
                TokenKind::AtVersion | TokenKind::Version => self.parse_version()?,
                TokenKind::LBrace => self.parse_formula_statement()?,
                TokenKind::Boolean(true) => self.parse_true_formula_statement()?,
                TokenKind::Dot => { self.advance(); }
                _ => {
                    let source = self.source_ref_at(self.peek().offset);
                    let facts = self.parse_triples_sequence()?;
                    self.add_facts(facts, source);
                    self.expect_dot()?;
                }
            }
        }
        Ok(self.doc)
    }

    fn parse_rdf12_statement(&mut self) -> Result<()> {
        let source = self.source_ref_at(self.peek().offset);
        match self.peek_kind() {
            TokenKind::AtPrefix | TokenKind::Prefix => return self.parse_prefix(),
            TokenKind::AtBase | TokenKind::Base => return self.parse_base(),
            TokenKind::AtVersion | TokenKind::Version => return self.parse_version(),
            TokenKind::Dot => { return Err(EyeronError::at("empty RDF statement", self.peek().offset)); }
            TokenKind::LBrace if matches!(self.profile.rdf_format(), Some(RdfFormat::Trig)) => {
                let triples = self.parse_graph_block()?;
                self.add_facts(triples, source.clone());
                if self.check(&TokenKind::Dot) { return Err(EyeronError::at("TriG graph blocks are not followed by '.'", self.peek().offset)); }
                return Ok(());
            }
            _ => {}
        }

        if matches!(self.profile.rdf_format(), Some(RdfFormat::Trig)) && self.peek_is_bare("graph") {
            self.advance();
            let (graph, generated) = self.parse_term()?;
            self.ensure_rdf_graph_label(&graph)?;
            if !generated.is_empty() { return Err(EyeronError::new("graph label cannot generate triples")); }
            let triples = self.parse_graph_block()?;
            self.add_fact(named_graph_formula(graph, triples), source.clone());
            if self.check(&TokenKind::Dot) { return Err(EyeronError::at("TriG graph blocks are not followed by '.'", self.peek().offset)); }
            return Ok(());
        }

        if self.check(&TokenKind::A) {
            return Err(EyeronError::at("'a' is only valid as an RDF predicate", self.peek().offset));
        }
        let (subject, generated) = self.parse_term()?;
        if self.check(&TokenKind::LBrace) {
            if !matches!(self.profile.rdf_format(), Some(RdfFormat::Trig)) {
                return Err(EyeronError::at("named graph blocks are only valid in TriG mode", self.peek().offset));
            }
            self.ensure_rdf_graph_label(&subject)?;
            if !generated.is_empty() { return Err(EyeronError::new("graph label cannot generate triples")); }
            let triples = self.parse_graph_block()?;
            self.add_fact(named_graph_formula(subject, triples), source.clone());
            if self.check(&TokenKind::Dot) { return Err(EyeronError::at("TriG graph blocks are not followed by '.'", self.peek().offset)); }
            return Ok(());
        }

        self.ensure_rdf_subject(&subject)?;
        let facts = self.parse_triples_sequence_from_subject_with_generated(subject, generated)?;
        self.add_facts(facts, source.clone());
        self.expect_dot()?;
        Ok(())
    }

    fn parse_rdf_line_document(&mut self) -> Result<()> {
        while !self.check(&TokenKind::Eof) {
            match self.peek_kind() {
                TokenKind::Dot => { return Err(EyeronError::at("empty N-Triples/N-Quads statement", self.peek().offset)); }
                TokenKind::AtPrefix | TokenKind::Prefix | TokenKind::AtBase | TokenKind::Base | TokenKind::AtVersion | TokenKind::Version => {
                    return Err(EyeronError::at("directives are not allowed in N-Triples/N-Quads", self.peek().offset));
                }
                _ => {}
            }

            let source = self.source_ref_at(self.peek().offset);
            if self.check(&TokenKind::A) { return Err(EyeronError::at("'a' is not N-Triples/N-Quads syntax", self.peek().offset)); }
            let (subject, mut generated) = self.parse_term()?;
            self.ensure_rdf_line_subject(&subject)?;
            if !generated.is_empty() { return Err(EyeronError::new("N-Triples/N-Quads subject cannot generate triples")); }

            if self.check(&TokenKind::A) { return Err(EyeronError::at("'a' is not N-Triples/N-Quads syntax", self.peek().offset)); }
            let (predicate, mut pred_generated) = self.parse_term()?;
            self.ensure_rdf_predicate(&predicate)?;
            if !pred_generated.is_empty() { return Err(EyeronError::new("N-Triples/N-Quads predicate cannot generate triples")); }

            if self.check(&TokenKind::A) { return Err(EyeronError::at("'a' is not N-Triples/N-Quads syntax", self.peek().offset)); }
            let (object, mut obj_generated) = self.parse_term()?;
            self.ensure_rdf_line_object(&object)?;
            if !obj_generated.is_empty() { return Err(EyeronError::new("N-Triples/N-Quads object cannot generate triples")); }

            let asserted = Triple::new(subject, predicate, object);
            generated.append(&mut pred_generated);
            generated.append(&mut obj_generated);
            generated.push(asserted);

            if matches!(self.profile.rdf_format(), Some(RdfFormat::NQuads)) && !self.check(&TokenKind::Dot) {
                let (graph, graph_generated) = self.parse_term()?;
                self.ensure_rdf_graph_label(&graph)?;
                if !graph_generated.is_empty() { return Err(EyeronError::new("graph label cannot generate triples")); }
                self.add_fact(named_graph_formula(graph, generated), source.clone());
            } else if matches!(self.profile.rdf_format(), Some(RdfFormat::NTriples)) && !self.check(&TokenKind::Dot) {
                return Err(EyeronError::at("N-Triples has too many terms before '.'", self.peek().offset));
            } else {
                self.add_facts(generated, source.clone());
            }
            self.expect_dot()?;
        }
        Ok(())
    }

    fn parse_graph_block(&mut self) -> Result<Vec<Triple>> {
        self.expect(TokenKind::LBrace)?;
        let mut triples = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            match self.peek_kind() {
                // The W3C TriG minimal-whitespace positive syntax case uses a
                // single leading statement separator in an anonymous graph
                // block.  Permit that compact form, but still reject the
                // negative "too many DOTs" cases such as `{ . . }`.
                TokenKind::Dot if triples.is_empty() => {
                    self.advance();
                    if self.check(&TokenKind::Dot) || self.check(&TokenKind::RBrace) || self.check(&TokenKind::Eof) {
                        return Err(EyeronError::at("extra '.' in TriG graph block", self.peek().offset));
                    }
                    continue;
                }
                TokenKind::Dot => {
                    return Err(EyeronError::at("extra '.' in TriG graph block", self.peek().offset));
                }
                TokenKind::AtPrefix | TokenKind::Prefix | TokenKind::AtBase | TokenKind::Base | TokenKind::AtVersion | TokenKind::Version => {
                    return Err(EyeronError::at("directives are not allowed inside TriG graph blocks", self.peek().offset));
                }
                _ => {
                    let mut parsed = self.parse_triples_sequence()?;
                    triples.append(&mut parsed);
                    if self.check(&TokenKind::Dot) { self.advance(); }
                    else if !self.check(&TokenKind::RBrace) {
                        return Err(EyeronError::at("expected '.' or '}' in graph block", self.peek().offset));
                    }
                }
            }
        }
        self.expect(TokenKind::RBrace)?;
        Ok(triples)
    }

    fn parse_prefix(&mut self) -> Result<()> {
        let directive = self.advance().clone();
        let at_style = matches!(directive.kind, TokenKind::AtPrefix);
        let t = self.advance().clone();
        let name = match t.kind {
            TokenKind::PName(p) => {
                if self.profile.is_rdf12() && !p.ends_with(':') {
                    return Err(EyeronError::at("RDF prefix name must end with ':'", t.offset));
                }
                p.strip_suffix(':').unwrap_or(&p).to_string()
            }
            _ => return Err(EyeronError::at("expected prefix name", t.offset)),
        };
        let iri_tok = self.advance().clone();
        let iri = match iri_tok.kind {
            TokenKind::Iri(i) => self.resolve_iri(&i),
            _ => return Err(EyeronError::at("expected prefix IRI", iri_tok.offset)),
        };
        validate_prefix_label(&name, t.offset)?;
        self.doc.prefixes.insert(name, iri);
        if self.profile.is_rdf12() {
            if at_style { self.expect_dot()?; }
            else if self.check(&TokenKind::Dot) { return Err(EyeronError::at("SPARQL-style PREFIX must not end with '.'", self.peek().offset)); }
        } else if self.check(&TokenKind::Dot) { self.advance(); }
        Ok(())
    }

    fn parse_base(&mut self) -> Result<()> {
        let directive = self.advance().clone();
        let at_style = matches!(directive.kind, TokenKind::AtBase);
        let iri_tok = self.advance().clone();
        let iri = match iri_tok.kind {
            TokenKind::Iri(i) => self.resolve_iri(&i),
            _ => return Err(EyeronError::at("expected base IRI", iri_tok.offset)),
        };
        self.doc.base_iri = Some(iri);
        if self.profile.is_rdf12() {
            if at_style { self.expect_dot()?; }
            else if self.check(&TokenKind::Dot) { return Err(EyeronError::at("SPARQL-style BASE must not end with '.'", self.peek().offset)); }
        } else if self.check(&TokenKind::Dot) { self.advance(); }
        Ok(())
    }

    fn parse_version(&mut self) -> Result<()> {
        self.advance();
        let version_tok = self.advance().clone();
        if self.profile.is_rdf12() {
            match version_tok.kind {
                TokenKind::String(_) | TokenKind::StringSingle(_) => {}
                _ => return Err(EyeronError::at("RDF VERSION requires a short string", version_tok.offset)),
            }
            if self.check(&TokenKind::Dot) { self.advance(); }
            return Ok(());
        }
        match version_tok.kind {
            TokenKind::String(_) | TokenKind::StringSingle(_) | TokenKind::StringLong(_) | TokenKind::StringLongSingle(_) | TokenKind::StringLongExtraQuote(_) | TokenKind::StringLongSingleExtraQuote(_) | TokenKind::PName(_) | TokenKind::Number(_) => {}
            _ => return Err(EyeronError::at("expected version value", version_tok.offset)),
        }
        if self.check(&TokenKind::Dot) { self.advance(); }
        Ok(())
    }

    fn parse_true_formula_statement(&mut self) -> Result<()> {
        let source = self.source_ref_at(self.peek().offset);
        self.advance();
        match self.peek_kind() {
            TokenKind::Arrow => {
                self.advance();
                let rhs = self.parse_forward_rule_rhs()?;
                self.doc.rules.push(Rule::new(Vec::new(), rhs, true).with_source(source.clone()));
            }
            TokenKind::BackArrow => {
                self.advance();
                let rhs = self.parse_formula_or_true()?;
                self.doc.rules.push(Rule::new(rhs, Vec::new(), false).with_source(source.clone()));
            }
            _ => {
                // `true false true .` is a valid N3 triple with boolean terms.
                let facts = self.parse_triples_sequence_from_subject(boolean_literal(true))?;
                self.add_facts(facts, source.clone());
            }
        }
        self.expect_dot()?;
        Ok(())
    }

    fn parse_formula_statement(&mut self) -> Result<()> {
        let source = self.source_ref_at(self.peek().offset);
        let lhs = self.parse_formula()?;
        match self.peek_kind() {
            TokenKind::Arrow => {
                self.advance();
                let rhs = self.parse_forward_rule_rhs()?;
                self.doc.rules.push(Rule::new(lhs, rhs, true).with_source(source.clone()));
            }
            TokenKind::BackArrow => {
                self.advance();
                let rhs = self.parse_formula_or_true()?;
                // `{ head } <= { body }` is a backward rule: use it
                // goal-directed when a forward premise asks for `head`.
                self.doc.rules.push(Rule::new(rhs, lhs, false).with_source(source.clone()));
            }
            _ => {
                let predicate = self.parse_verb()?;
                match predicate {
                    Term::Iri(ref iri) if iri == LOG_QUERY => {
                        let rhs = self.parse_formula_or_true()?;
                        self.doc.rules.push(Rule::new(lhs, rhs, true).with_source(source.clone()).with_query(true));
                    }
                    other => {
                        // Otherwise the leading formula is an ordinary term subject, e.g.
                        // `{ :A :B :C } a :Statement .` or a formula-valued predicate.
                        let subject = Term::formula(lhs);
                        let mut triples = Vec::new();
                        loop {
                            let (object, mut generated) = self.parse_term()?;
                            triples.push(Triple::new(subject.clone(), other.clone(), object));
                            triples.append(&mut generated);
                            if self.check(&TokenKind::Comma) { self.advance(); continue; }
                            break;
                        }
                        if self.check(&TokenKind::Semicolon) {
                            while self.check(&TokenKind::Semicolon) { self.advance(); }
                            if !matches!(self.peek_kind(), TokenKind::Dot | TokenKind::RBrace | TokenKind::RBracket) {
                                let next_pred = self.parse_verb()?;
                                loop {
                                    let (object, mut generated) = self.parse_term()?;
                                    triples.push(Triple::new(subject.clone(), next_pred.clone(), object));
                                    triples.append(&mut generated);
                                    if self.check(&TokenKind::Comma) { self.advance(); continue; }
                                    break;
                                }
                            }
                        }
                        self.add_facts(triples, source.clone());
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
            if matches!(self.peek_kind(), TokenKind::AtVersion | TokenKind::Version) {
                self.parse_version()?;
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
        self.parse_triples_sequence_from_subject_with_generated(subject, Vec::new())
    }

    fn parse_triples_sequence_from_subject_with_generated(&mut self, subject: Term, mut generated: Vec<Triple>) -> Result<Vec<Triple>> {
        let generated_standalone_ok = self.profile.is_rdf12()
            && !generated.is_empty()
            && (matches!(&subject, Term::Blank(_)) || generated_are_rdf12_reifications(&generated));
        let mut triples = Vec::new();
        triples.append(&mut generated);

        // N3 implication can appear as a statement inside a quoted formula,
        // for example `{ { ?x a :Cat } => { ?x a :Animal } . }`.
        // Store it as a first-class triple whose subject/object are formula terms;
        // the reasoner promotes derived implication triples to active rules.
        if self.check(&TokenKind::Arrow) || self.check(&TokenKind::BackArrow) {
            if self.profile.is_rdf12() { return Err(EyeronError::at("N3 implication is not RDF syntax", self.peek().offset)); }
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

        let predicate_object_triples = self.parse_predicate_object_list(subject.clone())?;
        if self.profile.is_rdf12() && predicate_object_triples.is_empty() && !generated_standalone_ok {
            return Err(EyeronError::at("expected RDF predicate-object list", self.peek().offset));
        }
        triples.extend(predicate_object_triples);
        Ok(triples)
    }

    fn parse_triples_sequence(&mut self) -> Result<Vec<Triple>> {
        if self.profile.is_rdf12() && self.check(&TokenKind::A) {
            return Err(EyeronError::at("'a' is only valid as an RDF predicate", self.peek().offset));
        }
        let (subject, generated) = self.parse_term()?;
        if self.profile.is_rdf12() { self.ensure_rdf_subject(&subject)?; }
        self.parse_triples_sequence_from_subject_with_generated(subject, generated)
    }

    fn parse_predicate_object_list(&mut self, subject: Term) -> Result<Vec<Triple>> {
        let mut triples = Vec::new();
        loop {
            if matches!(self.peek_kind(), TokenKind::Dot | TokenKind::RBrace | TokenKind::RBracket | TokenKind::RAnnotation) { break; }

            // N3 reverse-property path: `S <- :p O` means `O :p S`.
            if !self.profile.is_rdf12() && self.check(&TokenKind::Reverse) {
                self.advance();
                let predicate = self.parse_verb()?;
                loop {
                    let (object, mut generated) = self.parse_term()?;
                    let asserted = Triple::new(object, predicate.clone(), subject.clone());
                    triples.push(asserted.clone());
                    triples.append(&mut generated);
                    if self.profile.is_rdf12() { self.parse_rdf12_reifier_suffixes(&asserted, &mut triples)?; }
                    if self.check(&TokenKind::Comma) { self.advance(); continue; }
                    break;
                }
            }
            // N3 alternate spelling: `S is :p of O` means `O :p S`.
            else if !self.profile.is_rdf12() && self.peek_is_bare("is") {
                self.advance();
                let predicate = self.parse_verb()?;
                if !self.peek_is_bare("of") {
                    return Err(EyeronError::at("expected 'of' after 'is <predicate>'", self.peek().offset));
                }
                self.advance();
                loop {
                    let (object, mut generated) = self.parse_term()?;
                    let asserted = Triple::new(object, predicate.clone(), subject.clone());
                    triples.push(asserted.clone());
                    triples.append(&mut generated);
                    if self.profile.is_rdf12() { self.parse_rdf12_reifier_suffixes(&asserted, &mut triples)?; }
                    if self.check(&TokenKind::Comma) { self.advance(); continue; }
                    break;
                }
            }
            // N3 alternate spelling: `S has :p O` means `S :p O`.
            else {
                if !self.profile.is_rdf12() && self.peek_is_bare("has") { self.advance(); }
                let predicate = self.parse_verb()?;
                loop {
                    if self.profile.is_rdf12() && self.check(&TokenKind::A) {
                        return Err(EyeronError::at("'a' is only valid as an RDF predicate", self.peek().offset));
                    }
                    let (object, mut generated) = self.parse_term()?;
                    let asserted = Triple::new(subject.clone(), predicate.clone(), object);
                    triples.push(asserted.clone());
                    triples.append(&mut generated);
                    if self.profile.is_rdf12() { self.parse_rdf12_reifier_suffixes(&asserted, &mut triples)?; }
                    if self.check(&TokenKind::Comma) { self.advance(); continue; }
                    break;
                }
            }

            if self.check(&TokenKind::Semicolon) {
                while self.check(&TokenKind::Semicolon) { self.advance(); }
                if matches!(self.peek_kind(), TokenKind::Dot | TokenKind::RBrace | TokenKind::RBracket | TokenKind::RAnnotation) { break; }
                continue;
            }
            break;
        }
        Ok(triples)
    }

    fn parse_rdf12_reifier_suffixes(&mut self, asserted: &Triple, triples: &mut Vec<Triple>) -> Result<()> {
        let mut pending_reifier: Option<Term> = None;
        loop {
            if self.peek_is_reifier_token() {
                let reifier = self.parse_reifier_token()?;
                triples.push(Triple::new(
                    reifier.clone(),
                    Term::iri(RDF_REIFIES),
                    Term::formula(vec![asserted.clone()]),
                ));
                pending_reifier = Some(reifier);
                continue;
            }
            if self.check(&TokenKind::LAnnotation) {
                let reifier = if let Some(reifier) = pending_reifier.take() {
                    reifier
                } else {
                    let reifier = self.fresh_blank("reif");
                    triples.push(Triple::new(
                        reifier.clone(),
                        Term::iri(RDF_REIFIES),
                        Term::formula(vec![asserted.clone()]),
                    ));
                    reifier
                };
                self.parse_annotation_block(reifier, triples)?;
                continue;
            }
            break;
        }
        Ok(())
    }

    fn parse_annotation_block(&mut self, reifier: Term, triples: &mut Vec<Triple>) -> Result<()> {
        self.expect(TokenKind::LAnnotation)?;
        if self.check(&TokenKind::RAnnotation) {
            return Err(EyeronError::at("empty RDF 1.2 annotation block", self.peek().offset));
        }
        triples.extend(self.parse_predicate_object_list(reifier)?);
        self.expect(TokenKind::RAnnotation)?;
        Ok(())
    }

    fn peek_is_reifier_token(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::PName(p) if p.starts_with('~'))
    }

    fn parse_reifier_token(&mut self) -> Result<Term> {
        let tok = self.advance().clone();
        let (suffix, offset) = match tok.kind {
            TokenKind::PName(p) if p.starts_with('~') => (p[1..].to_string(), tok.offset),
            _ => return Err(EyeronError::at("expected RDF 1.2 reifier token", tok.offset)),
        };
        if suffix.is_empty() {
            match self.peek_kind() {
                TokenKind::Iri(_) | TokenKind::PName(_) | TokenKind::Blank(_) | TokenKind::A => {
                    let (term, generated) = self.parse_term()?;
                    if !generated.is_empty() { return Err(EyeronError::new("reifier term cannot generate triples")); }
                    match term {
                        Term::Iri(_) | Term::Blank(_) => Ok(term),
                        _ => Err(EyeronError::at("reifier must be an IRI or blank node", self.peek().offset)),
                    }
                }
                _ => Ok(self.fresh_blank("reif")),
            }
        } else if let Some(label) = suffix.strip_prefix("_:") {
            if label.is_empty() { return Err(EyeronError::at("empty blank-node reifier label", offset)); }
            Ok(Term::blank(label.to_string()))
        } else if suffix.contains(':') {
            Ok(Term::iri(self.expand_pname(&suffix, offset)?))
        } else {
            Err(EyeronError::at("reifier suffix must be an IRI, prefixed name, or blank node", offset))
        }
    }

    fn parse_verb(&mut self) -> Result<Term> {
        if self.check(&TokenKind::A) {
            self.advance();
            return Ok(Term::iri(RDF_TYPE));
        }
        if self.profile.is_rdf12() {
            let (term, generated) = self.parse_term()?;
            if !generated.is_empty() { return Err(EyeronError::new("RDF predicate cannot generate triples")); }
            self.ensure_rdf_predicate(&term)?;
            return Ok(term);
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
            TokenKind::Iri(i) => {
                let iri = if self.profile.is_line_syntax() { i } else { self.resolve_iri(&i) };
                Ok((Term::iri(iri), Vec::new()))
            }
            TokenKind::PName(p) => self.parse_pname_or_path(&p, tok.offset),
            TokenKind::Var(v) => {
                if self.profile.is_rdf12() { Err(EyeronError::at("variables are not allowed in RDF syntax", tok.offset)) }
                else { Ok((Term::var(v), Vec::new())) }
            }
            TokenKind::Blank(b) => {
                if self.profile.is_rdf12() { validate_blank_label(&b, tok.offset)?; }
                Ok((Term::blank(b), Vec::new()))
            }
            TokenKind::String(value) => self.finish_literal(value),
            TokenKind::StringSingle(value) | TokenKind::StringLong(value) | TokenKind::StringLongSingle(value) => {
                if self.profile.is_line_syntax() {
                    Err(EyeronError::at("N-Triples/N-Quads only allow short double-quoted string literals", tok.offset))
                } else {
                    self.finish_literal(value)
                }
            }
            TokenKind::StringLongExtraQuote(value) | TokenKind::StringLongSingleExtraQuote(value) => {
                if self.profile.is_rdf12() {
                    Err(EyeronError::at("extra quote before long string terminator is not RDF syntax", tok.offset))
                } else {
                    self.finish_literal(value)
                }
            }
            TokenKind::Number(value) => {
                if self.profile.is_line_syntax() {
                    Err(EyeronError::at("N-Triples/N-Quads do not allow bare numeric literals", tok.offset))
                } else if self.profile.is_rdf12() { rdf_number_literal(value, tok.offset).map(|t| (t, Vec::new())) }
                else { Ok((number_literal(value), Vec::new())) }
            }
            TokenKind::Boolean(value) => Ok((boolean_literal(value), Vec::new())),
            TokenKind::A => Ok((Term::iri(RDF_TYPE), Vec::new())),
            TokenKind::LBrace => {
                if self.profile.is_rdf12() { return Err(EyeronError::at("formula terms are not allowed in RDF syntax", tok.offset)); }
                self.pos -= 1;
                let triples = self.parse_formula()?;
                Ok((Term::formula(triples), Vec::new()))
            }
            TokenKind::LBracket => self.parse_blank_node_property_list(),
            TokenKind::LParen => self.parse_list(),
            TokenKind::LTriple => self.parse_triple_term(),
            other => Err(EyeronError::at(format!("expected term, got {:?}", other), tok.offset)),
        }
    }

    fn parse_triple_term(&mut self) -> Result<(Term, Vec<Triple>)> {
        let parenthesized = self.check(&TokenKind::LParen);
        if parenthesized { self.advance(); }

        if self.profile.is_line_syntax() && !parenthesized {
            return Err(EyeronError::at("N-Triples/N-Quads use parenthesized triple terms", self.peek().offset));
        }

        let (subject, mut generated) = self.parse_term()?;
        if self.profile.is_rdf12() {
            self.ensure_rdf_triple_subject(&subject)?;
            if !generated.is_empty() && !generated_are_rdf12_reifications(&generated) { return Err(EyeronError::new("compound subject is not allowed inside an RDF triple term")); }
        }
        let predicate = self.parse_verb()?;
        if self.profile.is_rdf12() { self.ensure_rdf_predicate(&predicate)?; }
        let (object, mut object_generated) = self.parse_term()?;
        if self.profile.is_rdf12() {
            self.ensure_rdf_triple_object(&object)?;
            if !object_generated.is_empty() && !generated_are_rdf12_reifications(&object_generated) { return Err(EyeronError::new("compound object is not allowed inside an RDF triple term")); }
        }
        generated.append(&mut object_generated);
        if parenthesized { self.expect(TokenKind::RParen)?; }
        let triple = Triple::new(subject, predicate, object);

        if self.profile.is_rdf12() && !parenthesized {
            let reifier = if self.peek_is_reifier_token() { self.parse_reifier_token()? } else { self.fresh_blank("reif") };
            self.expect(TokenKind::RTriple)?;
            generated.push(Triple::new(reifier.clone(), Term::iri(RDF_REIFIES), Term::formula(vec![triple])));
            Ok((reifier, generated))
        } else {
            self.expect(TokenKind::RTriple)?;
            Ok((Term::formula(vec![triple]), generated))
        }
    }

    fn finish_literal(&mut self, value: String) -> Result<(Term, Vec<Triple>)> {
        if !self.profile.is_rdf12() { validate_n3_literal_value(&value)?; }
        let mut lit = Literal::plain(value);
        if self.check(&TokenKind::HatHat) {
            self.advance();
            let (dt, generated) = self.parse_term()?;
            if !generated.is_empty() { return Err(EyeronError::new("datatype cannot generate triples")); }
            match dt {
                Term::Iri(i) => {
                    if self.profile.is_line_syntax() { self.ensure_absolute_iri(&i, "N-Triples/N-Quads datatype IRI")?; }
                    if self.profile.is_rdf12() && (i == RDF_LANG_STRING || i == RDF_DIR_LANG_STRING) {
                        return Err(EyeronError::new("rdf:langString and rdf:dirLangString require language-tag syntax"));
                    }
                    lit.datatype = Some(i)
                }
                _ => return Err(EyeronError::new("datatype must be an IRI")),
            }
        } else if let TokenKind::Lang(lang) = self.peek_kind() {
            let lang = lang.clone();
            self.advance();
            if self.profile.is_rdf12() {
                let (clean, _dir) = validate_lang_or_lang_dir(&lang, self.peek().offset)?;
                lit.language = Some(clean.to_ascii_lowercase());
            } else {
                lit.language = Some(lang);
            }
        }
        if self.profile.is_rdf12()
            && matches!(self.peek_kind(), TokenKind::PName(p) if p == "--ltr" || p == "--rtl")
        {
            return Err(EyeronError::at("base direction requires a language tag", self.peek().offset));
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
        if self.profile.emit_native_list_triples && !items.is_empty() {
            triples.push(Triple::new(list_term.clone(), Term::iri(RDF_FIRST), items[0].clone()));
            let rest = if items.len() == 1 { Term::iri(RDF_NIL) } else { Term::list(items[1..].to_vec()) };
            triples.push(Triple::new(list_term.clone(), Term::iri(RDF_REST), rest));
        }
        Ok((list_term, triples))
    }

    fn parse_pname_or_path(&mut self, pname: &str, offset: usize) -> Result<(Term, Vec<Triple>)> {
        if self.profile.is_rdf12() {
            if contains_unescaped_path_operator(pname) { return Err(EyeronError::at("property paths are not RDF syntax", offset)); }
            if !pname.contains(':') { return Err(EyeronError::at(format!("expected IRI or prefixed name, got {}", pname), offset)); }
        } else if contains_unescaped_path_operator(pname) {
            return self.parse_path_pname(pname, offset);
        }
        Ok((Term::iri(self.expand_pname(pname, offset)?), Vec::new()))
    }

    fn parse_path_pname(&mut self, pname: &str, offset: usize) -> Result<(Term, Vec<Triple>)> {
        let parts = split_unescaped_path(pname, offset)?;
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
            let local = if self.profile.is_rdf12() { decode_and_validate_pname_local(local, offset)? } else { local.to_string() };
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

    fn ensure_rdf_subject(&self, term: &Term) -> Result<()> {
        match term {
            Term::Iri(_) | Term::Blank(_) | Term::List(_) => Ok(()),
            Term::Literal(_) => Err(EyeronError::new("RDF subject cannot be a literal")),
            Term::Formula(_) => Err(EyeronError::new("RDF triple terms cannot be used as subjects")),
            Term::Var(_) => Err(EyeronError::new("variables are not allowed in RDF syntax")),
        }
    }

    fn ensure_rdf_line_subject(&self, term: &Term) -> Result<()> {
        match term {
            Term::Iri(iri) => self.ensure_absolute_iri(iri, "N-Triples/N-Quads subject"),
            Term::Blank(_) => Ok(()),
            _ => Err(EyeronError::new("N-Triples/N-Quads subject must be an IRI or blank node")),
        }
    }

    fn ensure_rdf_line_object(&self, term: &Term) -> Result<()> {
        match term {
            Term::Iri(iri) => self.ensure_absolute_iri(iri, "N-Triples/N-Quads object"),
            Term::Blank(_) | Term::Literal(_) => Ok(()),
            Term::Formula(triples) if triples.len() == 1 => Ok(()),
            _ => Err(EyeronError::new("N-Triples/N-Quads object must be an IRI, blank node, literal, or parenthesized triple term")),
        }
    }

    fn ensure_rdf_graph_label(&self, term: &Term) -> Result<()> {
        match term {
            Term::Iri(iri) => self.ensure_absolute_iri(iri, "graph label"),
            Term::Blank(_) => Ok(()),
            _ => Err(EyeronError::new("graph label must be an IRI or blank node")),
        }
    }

    fn ensure_rdf_predicate(&self, term: &Term) -> Result<()> {
        match term {
            Term::Iri(iri) => self.ensure_absolute_iri(iri, "RDF predicate"),
            _ => Err(EyeronError::new("RDF predicate must be an IRI")),
        }
    }

    fn ensure_rdf_triple_subject(&self, term: &Term) -> Result<()> {
        match term {
            Term::Iri(_) | Term::Blank(_) => Ok(()),
            Term::Formula(triples) if triples.len() == 1 => Ok(()),
            _ => Err(EyeronError::new("RDF triple-term subject must be an IRI, blank node, or triple term")),
        }
    }

    fn ensure_rdf_triple_object(&self, term: &Term) -> Result<()> {
        match term {
            Term::Iri(_) | Term::Blank(_) | Term::Literal(_) => Ok(()),
            Term::Formula(triples) if triples.len() == 1 => Ok(()),
            _ => Err(EyeronError::new("RDF triple-term object must be an IRI, blank node, literal, or triple term")),
        }
    }

    fn ensure_absolute_iri(&self, iri: &str, position: &str) -> Result<()> {
        if self.profile.is_line_syntax() && !has_uri_scheme(iri) {
            Err(EyeronError::new(format!("{} must be absolute", position)))
        } else {
            Ok(())
        }
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
    // RFC 3986 dot-segment removal must preserve significant empty path
    // segments.  Splitting on '/' and dropping empty segments incorrectly turns
    // paths such as //de//xyz into /de/xyz, which the W3C IRI-resolution tests
    // catch.
    let mut input = path.to_string();
    let mut output = String::new();
    while !input.is_empty() {
        if input.starts_with("../") { input.drain(..3); }
        else if input.starts_with("./") { input.drain(..2); }
        else if input.starts_with("/./") { input.replace_range(..3, "/"); }
        else if input == "/." { input.replace_range(..2, "/"); }
        else if input.starts_with("/../") {
            input.replace_range(..4, "/");
            remove_last_path_segment(&mut output);
        }
        else if input == "/.." {
            input.replace_range(..3, "/");
            remove_last_path_segment(&mut output);
        }
        else if input == "." || input == ".." { input.clear(); }
        else {
            let n = if let Some(stripped) = input.strip_prefix('/') {
                stripped.find('/').map(|idx| idx + 1).unwrap_or(input.len())
            } else {
                input.find('/').unwrap_or(input.len())
            };
            output.push_str(&input[..n]);
            input.drain(..n);
        }
    }
    output
}

fn remove_last_path_segment(output: &mut String) {
    if output.is_empty() { return; }
    if let Some(idx) = output.rfind('/') { output.truncate(idx); }
    else { output.clear(); }
}

fn validate_blank_label(value: &str, offset: usize) -> Result<()> {
    if value.is_empty() || value.ends_with('.') || value.contains("..") {
        return Err(EyeronError::at("invalid blank node label", offset));
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else { return Err(EyeronError::at("invalid blank node label", offset)); };
    if matches!(first, '-' | '.') || !(first == '_' || first.is_alphanumeric()) {
        return Err(EyeronError::at("invalid blank node label", offset));
    }
    if value.chars().any(|c| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '\\' | ':')) {
        return Err(EyeronError::at("invalid blank node label", offset));
    }
    Ok(())
}

fn validate_prefix_label(value: &str, offset: usize) -> Result<()> {
    if value.ends_with('.') || value.contains("..") {
        return Err(EyeronError::at("invalid prefix label", offset));
    }
    Ok(())
}

fn validate_lang_or_lang_dir(raw: &str, offset: usize) -> Result<(String, Option<&'static str>)> {
    let (lang, dir) = if let Some(lang) = raw.strip_suffix("--ltr") { (lang, Some("ltr")) }
        else if let Some(lang) = raw.strip_suffix("--rtl") { (lang, Some("rtl")) }
        else { (raw, None) };
    if lang.is_empty() || lang.contains("--") { return Err(EyeronError::at("invalid language tag", offset)); }
    let mut parts = lang.split('-');
    let Some(first) = parts.next() else { return Err(EyeronError::at("invalid language tag", offset)); };
    if first.is_empty() || first.len() > 8 || !first.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(EyeronError::at("invalid language tag", offset));
    }
    for part in parts {
        if part.is_empty() || part.len() > 8 || !part.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(EyeronError::at("invalid language tag", offset));
        }
    }
    Ok((lang.to_string(), dir))
}

fn decode_and_validate_pname_local(raw: &str, offset: usize) -> Result<String> {
    if raw.is_empty() { return Ok(String::new()); }
    if raw.starts_with('-') || raw.starts_with("\\-") || raw.starts_with('.') {
        return Err(EyeronError::at("invalid prefixed-name local part", offset));
    }
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let Some(esc) = chars.next() else { return Err(EyeronError::at("trailing prefixed-name escape", offset)); };
            if !"_~.-!$&'()*+,;=/?#@%".contains(esc) {
                return Err(EyeronError::at("invalid prefixed-name escape", offset));
            }
            out.push(esc);
        } else if ch == '%' {
            let a = chars.next();
            let b = chars.next();
            match (a, b) {
                (Some(a), Some(b)) if a.is_ascii_hexdigit() && b.is_ascii_hexdigit() => {
                    out.push('%'); out.push(a); out.push(b);
                }
                _ => return Err(EyeronError::at("invalid percent escape in prefixed-name local part", offset)),
            }
        } else {
            if matches!(ch, '~' | '^') { return Err(EyeronError::at("invalid prefixed-name local part", offset)); }
            out.push(ch);
        }
    }
    Ok(out)
}

fn validate_n3_literal_value(value: &str) -> Result<()> {
    if value.chars().any(|c| matches!(c, '\u{0000}' | '\u{FFFE}' | '\u{FFFF}')) {
        return Err(EyeronError::new("forbidden character in N3 string literal"));
    }
    Ok(())
}

fn contains_unescaped_path_operator(value: &str) -> bool {
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '!' || ch == '^' { return true; }
    }
    false
}

fn split_unescaped_path(value: &str, offset: usize) -> Result<Vec<(char, String)>> {
    let mut parts = Vec::<(char, String)>::new();
    let mut buf = String::new();
    let mut op = '!';
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            buf.push('\\');
            buf.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '!' || ch == '^' {
            if buf.is_empty() { return Err(EyeronError::at("empty path component", offset)); }
            parts.push((op, buf.clone()));
            buf.clear();
            op = ch;
        } else {
            buf.push(ch);
        }
    }
    if escaped { return Err(EyeronError::at("trailing path escape", offset)); }
    if buf.is_empty() { return Err(EyeronError::at("empty path component", offset)); }
    parts.push((op, buf));
    Ok(parts)
}

fn generated_are_rdf12_reifications(generated: &[Triple]) -> bool {
    generated.iter().all(|triple| matches!(&triple.p, Term::Iri(iri) if iri == RDF_REIFIES))
}

fn rdf_number_literal(value: String, offset: usize) -> Result<Term> {
    let datatype = if is_integer_lexical(&value) {
        "http://www.w3.org/2001/XMLSchema#integer"
    } else if is_decimal_lexical_rdf(&value) {
        "http://www.w3.org/2001/XMLSchema#decimal"
    } else if is_double_lexical(&value) {
        "http://www.w3.org/2001/XMLSchema#double"
    } else {
        return Err(EyeronError::at("invalid numeric literal", offset));
    };
    Ok(Term::Literal(Literal { value, datatype: Some(datatype.to_string()), language: None }))
}

fn is_integer_lexical(s: &str) -> bool {
    let s = s.strip_prefix('+').or_else(|| s.strip_prefix('-')).unwrap_or(s);
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn is_decimal_lexical_rdf(s: &str) -> bool {
    let s = s.strip_prefix('+').or_else(|| s.strip_prefix('-')).unwrap_or(s);
    if s.contains('e') || s.contains('E') { return false; }
    match s.split_once('.') {
        Some((a, b)) => (!a.is_empty() || !b.is_empty()) && a.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

fn is_double_lexical(s: &str) -> bool {
    let s2 = s.strip_prefix('+').or_else(|| s.strip_prefix('-')).unwrap_or(s);
    let Some(pos) = s2.find('e').or_else(|| s2.find('E')) else { return false; };
    let mant = &s2[..pos];
    let exp = &s2[pos + 1..];
    if exp.is_empty() { return false; }
    let exp = exp.strip_prefix('+').or_else(|| exp.strip_prefix('-')).unwrap_or(exp);
    if exp.is_empty() || !exp.chars().all(|c| c.is_ascii_digit()) { return false; }
    if mant.is_empty() { return false; }
    if let Some((a, b)) = mant.split_once('.') {
        (!a.is_empty() || !b.is_empty()) && a.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit())
    } else {
        mant.chars().all(|c| c.is_ascii_digit())
    }
}

const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
const RDF_DIR_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString";
