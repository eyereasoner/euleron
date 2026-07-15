use eyeron::{
    is_rdf_message_log, parse_n3, parse_rdf_message_log, reason_document, result_to_string,
    Document, ReasonerOptions, Rule, Term, Triple,
};
use std::collections::BTreeMap;

const LOG_IMPLIES: &str = "http://www.w3.org/2000/10/swap/log#implies";
const LOG_IMPLIED_BY: &str = "http://www.w3.org/2000/10/swap/log#impliedBy";

pub fn check_golden_documents(
    name: &str,
    sources: Vec<(&str, &str)>,
    golden: &str,
    golden_is_n3: bool,
) -> Result<(), String> {
    let mut doc = Document::new();
    for (label, source) in sources {
        let parsed = if is_rdf_message_log(source) {
            parse_rdf_message_log(source, None)
        } else {
            parse_n3(source, None)
        }
        .map_err(|err| format!("{name} failed to parse {label}: {err}"))?;
        doc.merge(parsed);
    }
    let result = reason_document(&doc, &ReasonerOptions::default());
    let out = result_to_string(&doc.prefixes, &result.derived);

    if !golden_is_n3 {
        for expected in stable_report_lines(golden) {
            if !out.contains(expected) {
                return Err(format!("{name} missing report line `{expected}`\nactual:\n{out}"));
            }
        }
        return Ok(());
    }

    let actual = parse_n3(&out, None)
        .map_err(|err| format!("{name} generated invalid N3: {err}\nactual:\n{out}"))?;
    let expected = parse_n3(golden, None)
        .map_err(|err| format!("{name} golden is invalid N3: {err}"))?;
    let actual_triples = document_triples(&actual);
    let expected_triples = document_triples(&expected);
    if graphs_isomorphic(&actual_triples, &expected_triples) { return Ok(()); }

    Err(format!(
        "{name} output is not isomorphic with its golden (actual {} triples, expected {})\nactual:\n{out}",
        actual_triples.len(),
        expected_triples.len(),
    ))
}

fn stable_report_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim).filter(|line| {
        !line.is_empty()
            && !line.starts_with('#')
            && !line.starts_with("- [")
    })
}

fn document_triples(doc: &Document) -> Vec<Triple> {
    let mut triples = doc.facts.clone();
    triples.extend(doc.rules.iter().map(rule_triple));
    triples
}

fn rule_triple(rule: &Rule) -> Triple {
    if rule.is_forward {
        Triple::new(
            Term::Formula(rule.premise.clone()),
            Term::iri(LOG_IMPLIES),
            Term::Formula(rule.conclusion.clone()),
        )
    } else {
        Triple::new(
            Term::Formula(rule.conclusion.clone()),
            Term::iri(LOG_IMPLIED_BY),
            Term::Formula(rule.premise.clone()),
        )
    }
}

fn graphs_isomorphic(actual: &[Triple], expected: &[Triple]) -> bool {
    if actual.len() != expected.len() { return false; }
    let mut used = vec![false; expected.len()];
    match_triples(
        actual,
        expected,
        0,
        &mut used,
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
    )
}

fn match_triples(
    actual: &[Triple],
    expected: &[Triple],
    index: usize,
    used: &mut [bool],
    blanks: &mut BTreeMap<String, String>,
    reverse_blanks: &mut BTreeMap<String, String>,
) -> bool {
    if index == actual.len() { return true; }
    for candidate in 0..expected.len() {
        if used[candidate] { continue; }
        let mut next_blanks = blanks.clone();
        let mut next_reverse = reverse_blanks.clone();
        if triple_matches(&actual[index], &expected[candidate], &mut next_blanks, &mut next_reverse) {
            used[candidate] = true;
            if match_triples(actual, expected, index + 1, used, &mut next_blanks, &mut next_reverse) {
                return true;
            }
            used[candidate] = false;
        }
    }
    false
}

fn triple_matches(
    actual: &Triple,
    expected: &Triple,
    blanks: &mut BTreeMap<String, String>,
    reverse_blanks: &mut BTreeMap<String, String>,
) -> bool {
    term_matches(&actual.s, &expected.s, blanks, reverse_blanks)
        && term_matches(&actual.p, &expected.p, blanks, reverse_blanks)
        && term_matches(&actual.o, &expected.o, blanks, reverse_blanks)
}

fn term_matches(
    actual: &Term,
    expected: &Term,
    blanks: &mut BTreeMap<String, String>,
    reverse_blanks: &mut BTreeMap<String, String>,
) -> bool {
    match (actual, expected) {
        (Term::Blank(a), Term::Blank(e)) => match (blanks.get(a), reverse_blanks.get(e)) {
            (Some(mapped), _) => mapped == e,
            (_, Some(mapped)) => mapped == a,
            (None, None) => {
                blanks.insert(a.clone(), e.clone());
                reverse_blanks.insert(e.clone(), a.clone());
                true
            }
        },
        (Term::List(a), Term::List(e)) => {
            a.len() == e.len()
                && a.iter().zip(e).all(|(a, e)| term_matches(a, e, blanks, reverse_blanks))
        }
        (Term::Formula(a), Term::Formula(e)) => {
            if a.len() != e.len() { return false; }
            let mut used = vec![false; e.len()];
            match_triples(a, e, 0, &mut used, blanks, reverse_blanks)
        }
        _ => actual == expected,
    }
}

pub fn progress_line(message: &str) {
    use std::io::Write;

    let line = format!("{message}\n");
    #[cfg(unix)]
    {
        if let Ok(mut stderr) = std::fs::OpenOptions::new().write(true).open("/dev/stderr") {
            let _ = stderr.write_all(line.as_bytes());
            let _ = stderr.flush();
            return;
        }
    }
    eprint!("{line}");
}

fn colour_enabled() -> bool {
    use std::io::IsTerminal;

    if std::env::var_os("NO_COLOR").is_some() { return false; }
    match std::env::var("CARGO_TERM_COLOR").as_deref() {
        Ok("always") => true,
        Ok("never") => false,
        _ => std::io::stderr().is_terminal(),
    }
}

pub fn green(text: &str) -> String {
    if colour_enabled() { format!("\x1b[32m{text}\x1b[0m") } else { text.to_string() }
}

pub fn red(text: &str) -> String {
    if colour_enabled() { format!("\x1b[31m{text}\x1b[0m") } else { text.to_string() }
}
