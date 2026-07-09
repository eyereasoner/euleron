use crate::ast::*;
use std::collections::{BTreeMap, BTreeSet};

pub fn result_to_string(prefixes: &BTreeMap<String, String>, triples: &[Triple]) -> String {
    let output_strings: Vec<String> = triples
        .iter()
        .filter_map(|t| match (&t.p, &t.o) {
            (Term::Iri(p), Term::Literal(l)) if p.as_str() == LOG_OUTPUT_STRING => Some(l.value.clone()),
            _ => None,
        })
        .collect();

    if !output_strings.is_empty() { return output_strings.join(""); }
    triples_to_n3(prefixes, triples)
}


pub fn triple_to_n3(prefixes: &BTreeMap<String, String>, t: &Triple) -> String {
    if is_implication_triple(t) {
        implication_to_n3(t, prefixes).trim_end().to_string()
    } else {
        format!(
            "{} {} {} .",
            term_to_n3(&t.s, prefixes, Position::Subject),
            term_to_n3(&t.p, prefixes, Position::Predicate),
            term_to_n3(&t.o, prefixes, Position::Object),
        )
    }
}

pub fn term_to_n3_object(term: &Term, prefixes: &BTreeMap<String, String>) -> String {
    term_to_n3(term, prefixes, Position::Object)
}

pub fn triples_to_n3(prefixes: &BTreeMap<String, String>, triples: &[Triple]) -> String {
    if triples.is_empty() { return String::new(); }
    let used = used_prefixes(prefixes, triples);
    let mut out = String::new();

    if used.contains("") {
        if let Some(base) = prefixes.get("") {
            out.push_str(&format!("@prefix : <{}> .\n", base));
        }
    }
    for p in &used {
        if p.is_empty() { continue; }
        if let Some(base) = prefixes.get(p) {
            out.push_str(&format!("@prefix {}: <{}> .\n", p, base));
        }
    }
    if !used.is_empty() { out.push('\n'); }

    for t in triples {
        if matches!((&t.p, &t.o), (Term::Iri(p), Term::Literal(_)) if p.as_str() == LOG_OUTPUT_STRING) { continue; }
        if is_implication_triple(t) {
            out.push_str(&implication_to_n3(t, prefixes));
            continue;
        }
        out.push_str(&format!(
            "{} {} {} .\n",
            term_to_n3(&t.s, prefixes, Position::Subject),
            term_to_n3(&t.p, prefixes, Position::Predicate),
            term_to_n3(&t.o, prefixes, Position::Object),
        ));
    }
    out
}

#[derive(Clone, Copy)]
enum Position { Subject, Predicate, Object }

fn term_to_n3(term: &Term, prefixes: &BTreeMap<String, String>, pos: Position) -> String {
    match term {
        Term::Iri(iri) if matches!(pos, Position::Predicate) && iri == RDF_TYPE => "a".to_string(),
        Term::Iri(iri) => compact_iri(iri, prefixes).unwrap_or_else(|| format!("<{}>", iri)),
        Term::Var(name) => format!("?{}", name),
        Term::Blank(name) => format!("_:{}", sanitize_blank(name)),
        Term::Literal(lit) => literal_to_n3(lit, prefixes),
        Term::List(items) => {
            let rendered = items
                .iter()
                .map(|item| term_to_n3(item, prefixes, Position::Object))
                .collect::<Vec<_>>()
                .join(" ");
            format!("({})", rendered)
        }
        Term::Formula(triples) => formula_to_n3(triples, prefixes, 0),
    }
}

fn is_implication_triple(t: &Triple) -> bool {
    matches!((&t.s, &t.p, &t.o), (Term::Formula(_), Term::Iri(p), Term::Formula(_)) if p == LOG_IMPLIES || p == LOG_IMPLIED_BY)
}

fn implication_to_n3(t: &Triple, prefixes: &BTreeMap<String, String>) -> String {
    match (&t.s, &t.o) {
        (Term::Formula(lhs), Term::Formula(rhs)) => {
            let op = match &t.p {
                Term::Iri(p) if p == LOG_IMPLIED_BY => "<=",
                _ => "=>",
            };
            format!("{} {} {} .\n", formula_to_n3(lhs, prefixes, 0), op, formula_to_n3(rhs, prefixes, 0))
        }
        _ => format!(
            "{} {} {} .\n",
            term_to_n3(&t.s, prefixes, Position::Subject),
            term_to_n3(&t.p, prefixes, Position::Predicate),
            term_to_n3(&t.o, prefixes, Position::Object),
        ),
    }
}

fn formula_to_n3(triples: &[Triple], prefixes: &BTreeMap<String, String>, indent: usize) -> String {
    if triples.is_empty() { return "true".to_string(); }
    let pad = " ".repeat(indent);
    let inner = " ".repeat(indent + 4);
    let mut out = String::new();
    out.push_str("{\n");
    for t in triples {
        if is_implication_triple(t) {
            let rendered = implication_to_n3(t, prefixes);
            for line in rendered.lines() {
                out.push_str(&inner);
                out.push_str(line);
                out.push('\n');
            }
        } else {
            out.push_str(&inner);
            out.push_str(&format!(
                "{} {} {} .\n",
                term_to_n3(&t.s, prefixes, Position::Subject),
                term_to_n3(&t.p, prefixes, Position::Predicate),
                term_to_n3(&t.o, prefixes, Position::Object),
            ));
        }
    }
    out.push_str(&pad);
    out.push('}');
    out
}

fn literal_to_n3(lit: &Literal, prefixes: &BTreeMap<String, String>) -> String {
    match lit.datatype.as_deref() {
        Some("http://www.w3.org/2001/XMLSchema#integer")
        | Some("http://www.w3.org/2001/XMLSchema#decimal")
        | Some("http://www.w3.org/2001/XMLSchema#double") => lit.value.clone(),
        Some("http://www.w3.org/2001/XMLSchema#boolean") if lit.value == "true" || lit.value == "false" => lit.value.clone(),
        Some(dt) => format!("\"{}\"^^{}", escape_string(&lit.value), compact_iri(dt, prefixes).unwrap_or_else(|| format!("<{}>", dt))),
        None => match &lit.language {
            Some(lang) => format!("\"{}\"@{}", escape_string(&lit.value), lang),
            None => format!("\"{}\"", escape_string(&lit.value)),
        },
    }
}

fn compact_iri(iri: &str, prefixes: &BTreeMap<String, String>) -> Option<String> {
    let mut best: Option<(&str, &str)> = None;
    for (prefix, base) in prefixes {
        if iri.starts_with(base) {
            let local = &iri[base.len()..];
            if !local.is_empty() && valid_local(local) {
                match best {
                    Some((_, best_base)) if best_base.len() >= base.len() => {}
                    _ => best = Some((prefix.as_str(), base.as_str())),
                }
            }
        }
    }
    best.map(|(prefix, base)| {
        let local = &iri[base.len()..];
        if prefix.is_empty() { format!(":{}", local) } else { format!("{}:{}", prefix, local) }
    })
}

fn used_prefixes(prefixes: &BTreeMap<String, String>, triples: &[Triple]) -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    for t in triples {
        collect_used_prefixes(&t.s, Position::Subject, prefixes, &mut used);
        if !is_implication_triple(t) {
            collect_used_prefixes(&t.p, Position::Predicate, prefixes, &mut used);
        }
        collect_used_prefixes(&t.o, Position::Object, prefixes, &mut used);
    }
    used
}

fn collect_used_prefixes(term: &Term, pos: Position, prefixes: &BTreeMap<String, String>, used: &mut BTreeSet<String>) {
    match term {
        Term::Iri(iri) => {
            if matches!(pos, Position::Predicate) && iri == RDF_TYPE { return; }
            if let Some((p, _)) = prefix_for_iri(iri, prefixes) { used.insert(p.to_string()); }
        }
        Term::Literal(lit) => {
            if let Some(dt) = &lit.datatype {
                if let Some((p, _)) = prefix_for_iri(dt, prefixes) { used.insert(p.to_string()); }
            }
        }
        Term::List(items) => {
            for item in items { collect_used_prefixes(item, Position::Object, prefixes, used); }
        }
        Term::Formula(triples) => {
            for t in triples {
                collect_used_prefixes(&t.s, Position::Subject, prefixes, used);
                if !is_implication_triple(t) {
                    collect_used_prefixes(&t.p, Position::Predicate, prefixes, used);
                }
                collect_used_prefixes(&t.o, Position::Object, prefixes, used);
            }
        }
        _ => {}
    }
}

fn prefix_for_iri<'a>(iri: &str, prefixes: &'a BTreeMap<String, String>) -> Option<(&'a str, &'a str)> {
    let mut best: Option<(&str, &str)> = None;
    for (prefix, base) in prefixes {
        if iri.starts_with(base) {
            let local = &iri[base.len()..];
            if !local.is_empty() && valid_local(local) {
                match best {
                    Some((_, best_base)) if best_base.len() >= base.len() => {}
                    _ => best = Some((prefix.as_str(), base.as_str())),
                }
            }
        }
    }
    best
}

fn valid_local(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && !s.starts_with('.')
        && !s.ends_with('.')
}

fn sanitize_blank(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

fn escape_string(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

pub fn document_debug(doc: &Document) -> String {
    let mut out = String::new();
    out.push_str("Document {\n");
    out.push_str("  prefixes:\n");
    for (k, v) in &doc.prefixes { out.push_str(&format!("    {:?}: {:?}\n", k, v)); }
    out.push_str(&format!("  base_iri: {:?}\n", doc.base_iri));
    out.push_str("  facts:\n");
    for t in &doc.facts { out.push_str(&format!("    {:?}\n", t)); }
    out.push_str("  rules:\n");
    for r in &doc.rules { out.push_str(&format!("    {:?}\n", r)); }
    out.push_str("}\n");
    out
}

pub fn rdf12_json(doc: &Document) -> String {
    let mut writer = RdfJsonWriter { quads: Vec::new(), blank_counter: 0 };
    writer.document(doc);
    format!("[{}]\n", writer.quads.join(","))
}

struct RdfJsonWriter {
    quads: Vec<String>,
    blank_counter: usize,
}

impl RdfJsonWriter {
    fn document(&mut self, doc: &Document) {
        for triple in &doc.facts {
            if let Some((graph, triples)) = named_graph_fact(triple) {
                let graph_json = self.graph_term_json(graph);
                for inner in triples {
                    self.triple(inner, &graph_json);
                }
            } else {
                let graph_json = default_graph_json();
                self.triple(triple, &graph_json);
            }
        }
    }

    fn triple(&mut self, triple: &Triple, graph_json: &str) {
        let s = self.term_json(&triple.s, JsonPosition::Subject, graph_json);
        let p = self.term_json(&triple.p, JsonPosition::Predicate, graph_json);
        let o = self.term_json(&triple.o, JsonPosition::Object, graph_json);
        self.quads.push(quad_json(&s, &p, &o, graph_json));
    }

    fn term_json(&mut self, term: &Term, position: JsonPosition, graph_json: &str) -> String {
        match term {
            Term::Iri(iri) => named_node_json(iri),
            Term::Blank(id) => blank_node_json(id),
            Term::Literal(lit) => literal_json(lit),
            Term::List(items) => self.list_json(items, graph_json),
            Term::Formula(triples) if triples.len() == 1 && !matches!(position, JsonPosition::Predicate | JsonPosition::Graph) => {
                let triple = &triples[0];
                let s = self.term_json(&triple.s, JsonPosition::Subject, graph_json);
                let p = self.term_json(&triple.p, JsonPosition::Predicate, graph_json);
                let o = self.term_json(&triple.o, JsonPosition::Object, graph_json);
                quad_json(&s, &p, &o, &default_graph_json())
            }
            Term::Formula(_) => blank_node_json(&self.fresh_blank_id("unsupportedFormula")),
            Term::Var(name) => blank_node_json(&format!("var_{}", sanitize_blank(name))),
        }
    }

    fn graph_term_json(&mut self, term: &Term) -> String {
        match term {
            Term::Iri(_) | Term::Blank(_) => self.term_json(term, JsonPosition::Graph, &default_graph_json()),
            _ => blank_node_json(&self.fresh_blank_id("graph")),
        }
    }

    fn list_json(&mut self, items: &[Term], graph_json: &str) -> String {
        if items.is_empty() { return named_node_json(RDF_NIL); }
        let nodes: Vec<String> = (0..items.len()).map(|_| self.fresh_blank_id("rdfList")).collect();
        for (idx, item) in items.iter().enumerate() {
            let subject = blank_node_json(&nodes[idx]);
            let value = self.term_json(item, JsonPosition::Object, graph_json);
            let rest = if idx + 1 < nodes.len() { blank_node_json(&nodes[idx + 1]) } else { named_node_json(RDF_NIL) };
            self.quads.push(quad_json(&subject, &named_node_json(RDF_FIRST), &value, graph_json));
            self.quads.push(quad_json(&subject, &named_node_json(RDF_REST), &rest, graph_json));
        }
        blank_node_json(&nodes[0])
    }

    fn fresh_blank_id(&mut self, prefix: &str) -> String {
        self.blank_counter += 1;
        format!("{}{}", prefix, self.blank_counter)
    }
}

#[derive(Clone, Copy)]
enum JsonPosition { Subject, Predicate, Object, Graph }

fn named_graph_fact(triple: &Triple) -> Option<(&Term, &[Triple])> {
    match (&triple.p, &triple.o) {
        (Term::Iri(p), Term::Formula(triples)) if p == LOG_NAME_OF => Some((&triple.s, triples)),
        _ => None,
    }
}

fn named_node_json(value: &str) -> String {
    format!("{{\"termType\":\"NamedNode\",\"value\":\"{}\"}}", escape_json(value))
}

fn blank_node_json(value: &str) -> String {
    format!("{{\"termType\":\"BlankNode\",\"value\":\"{}\"}}", escape_json(value))
}

fn literal_json(lit: &Literal) -> String {
    let (language, datatype) = match lit.language.as_deref() {
        Some(lang) if lang.contains("--") => (lang, "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString"),
        Some(lang) => (lang, "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"),
        None => ("", lit.datatype.as_deref().unwrap_or("http://www.w3.org/2001/XMLSchema#string")),
    };
    format!(
        "{{\"termType\":\"Literal\",\"value\":\"{}\",\"language\":\"{}\",\"datatype\":{}}}",
        escape_json(&lit.value),
        escape_json(language),
        named_node_json(datatype),
    )
}

fn default_graph_json() -> String {
    "{\"termType\":\"DefaultGraph\",\"value\":\"\"}".to_string()
}

fn quad_json(subject: &str, predicate: &str, object: &str, graph: &str) -> String {
    format!("{{\"termType\":\"Quad\",\"value\":\"\",\"subject\":{},\"predicate\":{},\"object\":{},\"graph\":{}}}", subject, predicate, object, graph)
}

fn escape_json(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if c <= '\u{001F}' => out.push_str(&format!("\\u{:04X}", c as u32)),
            other => out.push(other),
        }
    }
    out
}
