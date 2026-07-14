//! Euleron core Notation3 reasoner.
//!
//! This crate intentionally keeps the public API small: parse one or more N3
//! sources, run forward-chaining rules, and render newly-derived triples.

pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod rdf_compat;
pub mod printing;
pub mod proof;
pub mod reasoner;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub use ast::{Document, Literal, Rule, SourceRef, Term, Triple};
pub use error::{EuleronError, Result};
pub use parser::{is_rdf_message_log, parse_n3, parse_n3_with_source, parse_rdf_message_log};
pub use rdf_compat::{parse_rdf12, RdfFormat};
pub use printing::{document_debug, rdf12_json, rdf_result_to_string, result_to_string, triples_to_n3, triples_to_trig};
pub use proof::proof_to_n3;
pub use reasoner::{
    reason as reason_document, CompletionStatus, ReasonerError, ReasonerLimit, ReasonerOptions,
    ReasonerResult, ReasonerStatistics,
};

/// Parse an N3 string, run the forward reasoner, and return the N3 output for
/// newly-derived triples.
pub fn reason(input: &str) -> Result<String> {
    let doc = if is_rdf_message_log(input) {
        parse_rdf_message_log(input, None)?
    } else {
        parse_n3(input, None)?
    };
    let result = reason_document(&doc, &ReasonerOptions::default());
    if let Some(summary) = result.incomplete_summary() {
        return Err(EuleronError::new(summary));
    }
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
    #[test]
    fn rdf_message_log_preserves_utf8_literals() {
        let input = r#"
            PREFIX : <http://example.org/>
            PREFIX log: <http://www.w3.org/2000/10/swap/log#>
            VERSION "1.2-messages"
            MESSAGE
            :reading :label "8.0°C" .
        "#;
        let doc = parse_rdf_message_log(input, None).unwrap();

        fn term_has_literal(term: &Term, value: &str) -> bool {
            match term {
                Term::Literal(lit) => lit.value == value,
                Term::List(items) => items.iter().any(|item| term_has_literal(item, value)),
                Term::Formula(triples) => triples.iter().any(|t| {
                    term_has_literal(&t.s, value) || term_has_literal(&t.p, value) || term_has_literal(&t.o, value)
                }),
                _ => false,
            }
        }

        assert!(
            doc.facts.iter().any(|t| {
                term_has_literal(&t.s, "8.0°C")
                    || term_has_literal(&t.p, "8.0°C")
                    || term_has_literal(&t.o, "8.0°C")
            }),
            "{:#?}",
            doc.facts
        );
    }

    #[test]
    fn rdf_message_log_accepts_turtle_style_directives() {
        let input = r#"
            @version "1.1-messages" .
            <http://example.org/one> <http://example.org/value> "first" .
            @message .
            <http://example.org/two> <http://example.org/value> "second" .
            @message .
        "#;
        assert!(is_rdf_message_log(input));
        let doc = parse_rdf_message_log(input, None).unwrap();
        assert!(doc.facts.iter().any(|triple| {
            matches!(&triple.o, Term::Formula(facts) if facts.iter().any(|fact| {
                matches!(&fact.o, Term::Literal(literal) if literal.value == "first")
            }))
        }));
    }

}
