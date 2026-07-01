//! Eyeron core Notation3 reasoner.
//!
//! This crate intentionally keeps the public API small: parse one or more N3
//! sources, run forward-chaining rules, and render newly-derived triples.

pub mod ast;
pub mod error;
pub mod incremental;
pub mod lexer;
pub mod parser;
pub mod printing;
pub mod reasoner;

pub use ast::{Document, Literal, Rule, Term, Triple};
pub use error::{EyeronError, Result};
pub use incremental::{blank as rdf_blank, iri as rdf_iri, literal as rdf_literal, var as rdf_var, Delta, IncrementalReasoner, Quad, RdfTerm};
pub use parser::parse_n3;
pub use printing::{document_debug, result_to_string, triples_to_n3};
pub use reasoner::{reason as reason_document, ReasonerOptions, ReasonerResult};

/// Parse an N3 string, run the forward reasoner, and return the N3 output for
/// newly-derived triples.
pub fn reason(input: &str) -> Result<String> {
    let doc = parse_n3(input, None)?;
    let result = reason_document(&doc, &ReasonerOptions::default());
    Ok(result_to_string(&doc.prefixes, &result.derived))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socrates() {
        let input = r#"
            @prefix : <http://example.org/> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            :Socrates a :Human .
            :Human rdfs:subClassOf :Mortal .
            { ?s a ?a . ?a rdfs:subClassOf ?b . } => { ?s a ?b . } .
        "#;
        let out = reason(input).unwrap();
        assert!(out.contains(":Socrates a :Mortal"), "{}", out);
    }

    #[test]
    fn lists_are_preserved_for_builtins() {
        let input = r#"
            @prefix : <http://example.org/> .
            :x :has (1 2) .
        "#;
        let doc = parse_n3(input, None).unwrap();
        assert!(doc.facts.iter().any(|t| matches!(&t.o, Term::List(items) if items.len() == 2)));
    }

    #[test]
    fn math_sum_and_greater_than() {
        let input = r#"
            @prefix : <http://example.org/> .
            @prefix math: <http://www.w3.org/2000/10/swap/math#> .
            :x :generation 0 .
            {
              :x :generation ?g .
              (?g 1) math:sum ?g1 .
              ?g1 math:greaterThan 0 .
            } => {
              :x :next ?g1 .
            } .
        "#;
        let out = reason(input).unwrap();
        assert!(out.contains(":x :next 1"), "{}", out);
    }

    #[test]
    fn log_query_is_parsed_as_rule() {
        let input = r#"
            @prefix : <http://example.org/> .
            :x :text "ok" .
            { :x :text ?text } log:query { :x log:outputString ?text } .
        "#;
        let out = reason(input).unwrap();
        assert_eq!(out, "ok");
    }
}
