use crate::ast::Document;
use crate::error::Result;
use crate::parser;

/// RDF syntax families accepted by the RDF compatibility layer.
///
/// Eyeron's core language is N3.  These formats are kept behind this module so
/// callers can import RDF 1.x/Turtle/TriG/N-Triples/N-Quads input without making
/// the public parser API look like multiple peer parser languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RdfFormat {
    Turtle,
    NTriples,
    NQuads,
    Trig,
}

impl RdfFormat {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "turtle" | "ttl" => Some(Self::Turtle),
            "n-triples" | "ntriples" | "nt" => Some(Self::NTriples),
            "n-quads" | "nquads" | "nq" => Some(Self::NQuads),
            "trig" => Some(Self::Trig),
            _ => None,
        }
    }
}

/// Parse RDF 1.x compatible syntaxes into Eyeron's N3 document model.
///
/// This is intentionally a compatibility adapter around the existing, W3C-tested
/// RDF parser behavior.  The N3 entry point remains `parser::parse_n3`; RDF
/// syntaxes enter through this module and are materialized as the same internal
/// `Document` representation used by the reasoner.
pub fn parse_rdf12(input: &str, base_iri: Option<&str>, format: RdfFormat) -> Result<Document> {
    parser::parse_rdf12_compat(input, base_iri, format)
}
