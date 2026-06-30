use std::collections::BTreeMap;
use std::fmt;

pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
pub const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
pub const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
pub const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
pub const LOG_OUTPUT_STRING: &str = "http://www.w3.org/2000/10/swap/log#outputString";
pub const LOG_QUERY: &str = "http://www.w3.org/2000/10/swap/log#query";
pub const LOG_EQUAL_TO: &str = "http://www.w3.org/2000/10/swap/log#equalTo";
pub const LOG_NOT_EQUAL_TO: &str = "http://www.w3.org/2000/10/swap/log#notEqualTo";
pub const LOG_IMPLIES: &str = "http://www.w3.org/2000/10/swap/log#implies";
/// Internal predicate used to preserve N3 `<=` rule polarity.
/// It is printed as `<=` and promoted to a backward rule, not exposed as a normal RDF predicate.
pub const LOG_IMPLIED_BY: &str = "urn:eyeling:impliedBy";
/// Internal marker used for rule conclusions like `=> ?F`, where the
/// RHS resolves to a quoted formula whose contents should be unquoted.
pub const EYELING_UNQUOTE: &str = "urn:eyeling:unquote";
pub const LOG_COLLECT_ALL_IN: &str = "http://www.w3.org/2000/10/swap/log#collectAllIn";
pub const LOG_FOR_ALL_IN: &str = "http://www.w3.org/2000/10/swap/log#forAllIn";
pub const LOG_CONCLUSION: &str = "http://www.w3.org/2000/10/swap/log#conclusion";
pub const LOG_CONJUNCTION: &str = "http://www.w3.org/2000/10/swap/log#conjunction";
pub const LOG_NOT_INCLUDES: &str = "http://www.w3.org/2000/10/swap/log#notIncludes";
pub const LOG_URI: &str = "http://www.w3.org/2000/10/swap/log#uri";
pub const OWL_SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
pub const MATH_SUM: &str = "http://www.w3.org/2000/10/swap/math#sum";
pub const MATH_GREATER_THAN: &str = "http://www.w3.org/2000/10/swap/math#greaterThan";
pub const MATH_LESS_THAN: &str = "http://www.w3.org/2000/10/swap/math#lessThan";
pub const MATH_NOT_GREATER_THAN: &str = "http://www.w3.org/2000/10/swap/math#notGreaterThan";
pub const MATH_NOT_LESS_THAN: &str = "http://www.w3.org/2000/10/swap/math#notLessThan";
pub const MATH_EQUAL_TO: &str = "http://www.w3.org/2000/10/swap/math#equalTo";
pub const MATH_NOT_EQUAL_TO: &str = "http://www.w3.org/2000/10/swap/math#notEqualTo";
pub const MATH_DIFFERENCE: &str = "http://www.w3.org/2000/10/swap/math#difference";
pub const MATH_PRODUCT: &str = "http://www.w3.org/2000/10/swap/math#product";
pub const MATH_QUOTIENT: &str = "http://www.w3.org/2000/10/swap/math#quotient";
pub const MATH_INTEGER_QUOTIENT: &str = "http://www.w3.org/2000/10/swap/math#integerQuotient";
pub const MATH_REMAINDER: &str = "http://www.w3.org/2000/10/swap/math#remainder";
pub const MATH_EXPONENTIATION: &str = "http://www.w3.org/2000/10/swap/math#exponentiation";
pub const MATH_NEGATION: &str = "http://www.w3.org/2000/10/swap/math#negation";
pub const MATH_ABSOLUTE_VALUE: &str = "http://www.w3.org/2000/10/swap/math#absoluteValue";
pub const MATH_ROUNDED: &str = "http://www.w3.org/2000/10/swap/math#rounded";
pub const MATH_SIN: &str = "http://www.w3.org/2000/10/swap/math#sin";
pub const MATH_COS: &str = "http://www.w3.org/2000/10/swap/math#cos";
pub const MATH_TAN: &str = "http://www.w3.org/2000/10/swap/math#tan";
pub const MATH_ASIN: &str = "http://www.w3.org/2000/10/swap/math#asin";
pub const MATH_ACOS: &str = "http://www.w3.org/2000/10/swap/math#acos";
pub const MATH_ATAN: &str = "http://www.w3.org/2000/10/swap/math#atan";
pub const MATH_SINH: &str = "http://www.w3.org/2000/10/swap/math#sinh";
pub const MATH_COSH: &str = "http://www.w3.org/2000/10/swap/math#cosh";
pub const MATH_TANH: &str = "http://www.w3.org/2000/10/swap/math#tanh";
pub const MATH_DEGREES: &str = "http://www.w3.org/2000/10/swap/math#degrees";
pub const STRING_LESS_THAN: &str = "http://www.w3.org/2000/10/swap/string#lessThan";
pub const STRING_GREATER_THAN: &str = "http://www.w3.org/2000/10/swap/string#greaterThan";
pub const STRING_NOT_LESS_THAN: &str = "http://www.w3.org/2000/10/swap/string#notLessThan";
pub const STRING_NOT_GREATER_THAN: &str = "http://www.w3.org/2000/10/swap/string#notGreaterThan";
pub const STRING_CONCATENATION: &str = "http://www.w3.org/2000/10/swap/string#concatenation";
pub const STRING_CONTAINS: &str = "http://www.w3.org/2000/10/swap/string#contains";
pub const STRING_CONTAINS_IGNORING_CASE: &str = "http://www.w3.org/2000/10/swap/string#containsIgnoringCase";
pub const STRING_ENDS_WITH: &str = "http://www.w3.org/2000/10/swap/string#endsWith";
pub const STRING_STARTS_WITH: &str = "http://www.w3.org/2000/10/swap/string#startsWith";
pub const STRING_EQUAL_IGNORING_CASE: &str = "http://www.w3.org/2000/10/swap/string#equalIgnoringCase";
pub const STRING_NOT_EQUAL_IGNORING_CASE: &str = "http://www.w3.org/2000/10/swap/string#notEqualIgnoringCase";
pub const STRING_FORMAT: &str = "http://www.w3.org/2000/10/swap/string#format";
pub const STRING_MATCHES: &str = "http://www.w3.org/2000/10/swap/string#matches";
pub const STRING_NOT_MATCHES: &str = "http://www.w3.org/2000/10/swap/string#notMatches";
pub const STRING_REPLACE: &str = "http://www.w3.org/2000/10/swap/string#replace";
pub const STRING_SCRAPE: &str = "http://www.w3.org/2000/10/swap/string#scrape";
pub const LIST_APPEND: &str = "http://www.w3.org/2000/10/swap/list#append";
pub const LIST_ITERATE: &str = "http://www.w3.org/2000/10/swap/list#iterate";
pub const LIST_MAP: &str = "http://www.w3.org/2000/10/swap/list#map";
pub const LIST_FIRST_REST: &str = "http://www.w3.org/2000/10/swap/list#firstRest";
pub const LIST_REVERSE: &str = "http://www.w3.org/2000/10/swap/list#reverse";
pub const LIST_SORT: &str = "http://www.w3.org/2000/10/swap/list#sort";
pub const LIST_NOT_MEMBER: &str = "http://www.w3.org/2000/10/swap/list#notMember";
pub const LIST_FIRST: &str = "http://www.w3.org/2000/10/swap/list#first";
pub const LIST_REST: &str = "http://www.w3.org/2000/10/swap/list#rest";
pub const LIST_LAST: &str = "http://www.w3.org/2000/10/swap/list#last";
pub const LIST_LENGTH: &str = "http://www.w3.org/2000/10/swap/list#length";
pub const LIST_MEMBER: &str = "http://www.w3.org/2000/10/swap/list#member";
pub const LIST_IN: &str = "http://www.w3.org/2000/10/swap/list#in";
pub const LIST_MEMBER_AT: &str = "http://www.w3.org/2000/10/swap/list#memberAt";
pub const LIST_REMOVE: &str = "http://www.w3.org/2000/10/swap/list#remove";

pub const TIME_YEAR: &str = "http://www.w3.org/2000/10/swap/time#year";
pub const TIME_MONTH: &str = "http://www.w3.org/2000/10/swap/time#month";
pub const TIME_DAY: &str = "http://www.w3.org/2000/10/swap/time#day";
pub const TIME_HOUR: &str = "http://www.w3.org/2000/10/swap/time#hour";
pub const TIME_MINUTE: &str = "http://www.w3.org/2000/10/swap/time#minute";
pub const TIME_SECOND: &str = "http://www.w3.org/2000/10/swap/time#second";
pub const TIME_TIME_ZONE: &str = "http://www.w3.org/2000/10/swap/time#timeZone";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Literal {
    pub value: String,
    pub datatype: Option<String>,
    pub language: Option<String>,
}

impl Literal {
    pub fn plain(value: impl Into<String>) -> Self {
        Self { value: value.into(), datatype: None, language: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Term {
    Iri(String),
    Var(String),
    Blank(String),
    Literal(Literal),
    List(Vec<Term>),
    Formula(Vec<Triple>),
}

impl Term {
    pub fn iri(value: impl Into<String>) -> Self { Self::Iri(value.into()) }
    pub fn var(value: impl Into<String>) -> Self { Self::Var(value.into()) }
    pub fn blank(value: impl Into<String>) -> Self { Self::Blank(value.into()) }
    pub fn literal(value: impl Into<String>) -> Self { Self::Literal(Literal::plain(value)) }
    pub fn list(items: Vec<Term>) -> Self { Self::List(items) }
    pub fn formula(triples: Vec<Triple>) -> Self { Self::Formula(triples) }

    pub fn is_variable(&self) -> bool { matches!(self, Term::Var(_)) }
    pub fn is_ground(&self) -> bool {
        match self {
            Term::Var(_) => false,
            Term::List(items) => items.iter().all(Term::is_ground),
            Term::Formula(triples) => triples.iter().all(Triple::is_ground),
            _ => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Triple {
    pub s: Term,
    pub p: Term,
    pub o: Term,
}

impl Triple {
    pub fn new(s: Term, p: Term, o: Term) -> Self { Self { s, p, o } }

    pub fn is_ground(&self) -> bool {
        self.s.is_ground() && self.p.is_ground() && self.o.is_ground()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub premise: Vec<Triple>,
    pub conclusion: Vec<Triple>,
    pub is_forward: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub prefixes: BTreeMap<String, String>,
    pub base_iri: Option<String>,
    pub facts: Vec<Triple>,
    pub rules: Vec<Rule>,
}

impl Document {
    pub fn new() -> Self {
        Self { prefixes: default_prefixes(), base_iri: None, facts: Vec::new(), rules: Vec::new() }
    }

    pub fn merge(&mut self, other: Document) {
        for (k, v) in other.prefixes { self.prefixes.insert(k, v); }
        if self.base_iri.is_none() { self.base_iri = other.base_iri; }
        self.facts.extend(other.facts);
        self.rules.extend(other.rules);
    }
}

impl Default for Document {
    fn default() -> Self { Self::new() }
}

pub fn default_prefixes() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    m.insert("rdf".to_string(), "http://www.w3.org/1999/02/22-rdf-syntax-ns#".to_string());
    m.insert("rdfs".to_string(), "http://www.w3.org/2000/01/rdf-schema#".to_string());
    m.insert("xsd".to_string(), "http://www.w3.org/2001/XMLSchema#".to_string());
    m.insert("log".to_string(), "http://www.w3.org/2000/10/swap/log#".to_string());
    m.insert("owl".to_string(), "http://www.w3.org/2002/07/owl#".to_string());
    m.insert("math".to_string(), "http://www.w3.org/2000/10/swap/math#".to_string());
    m.insert("string".to_string(), "http://www.w3.org/2000/10/swap/string#".to_string());
    m.insert("list".to_string(), "http://www.w3.org/2000/10/swap/list#".to_string());
    m.insert("time".to_string(), "http://www.w3.org/2000/10/swap/time#".to_string());
    m.insert("dt".to_string(), "https://eyereasoner.github.io/eyeling/datatype#".to_string());
    m.insert("genid".to_string(), "https://eyereasoner.github.io/.well-known/genid/".to_string());
    m
}

impl fmt::Display for Triple {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} {:?} {:?}", self.s, self.p, self.o)
    }
}
