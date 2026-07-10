use wasm_bindgen::prelude::*;

use crate::error::{EyeronError, Result};
use crate::parser::{is_rdf_message_log, parse_n3, parse_n3_with_source, parse_rdf_message_log};
use crate::printing::{rdf_result_to_string, result_to_string};
use crate::proof::proof_to_n3;
use crate::rdf_compat::{parse_rdf12, RdfFormat};
use crate::reasoner::{reason as reason_document, ReasonerOptions};

#[wasm_bindgen(js_name = version)]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[wasm_bindgen(js_name = reasonWithOptions)]
pub fn reason_with_options(input: &str, proof: bool, rdf: bool, rdf_format: &str) -> std::result::Result<String, JsValue> {
    run(input, "", proof, rdf, rdf_format).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = reason)]
pub fn reason(input: &str) -> std::result::Result<String, JsValue> {
    run(input, "", false, false, "n3").map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = reasonWithData)]
pub fn reason_with_data(program: &str, data: &str, proof: bool, rdf: bool, rdf_format: &str) -> std::result::Result<String, JsValue> {
    run(program, data, proof, rdf, rdf_format).map_err(|err| JsValue::from_str(&err.to_string()))
}

fn run(program: &str, data: &str, proof: bool, rdf: bool, rdf_format: &str) -> Result<String> {
    let mut doc = crate::Document::new();
    if data.trim().is_empty() {
        let parsed = parse_source(program, proof, rdf, rdf_format, "playground")?;
        doc.merge(parsed);
    } else {
        let data_doc = parse_source(data, false, rdf, rdf_format, "playground-data")?;
        doc.merge(data_doc);
        let program_doc = parse_source(program, proof, false, "n3", "playground")?;
        doc.merge(program_doc);
    }

    let result = reason_document(&doc, &ReasonerOptions { proof, ..ReasonerOptions::default() });
    if proof {
        Ok(proof_to_n3(&doc.prefixes, &result))
    } else if rdf {
        Ok(rdf_result_to_string(&doc.prefixes, &result.derived))
    } else {
        Ok(result_to_string(&doc.prefixes, &result.derived))
    }
}

fn parse_source(input: &str, proof: bool, rdf: bool, rdf_format: &str, label: &str) -> Result<crate::Document> {
    if is_rdf_message_log(input) {
        parse_rdf_message_log(input, None)
    } else if rdf {
        let format = rdf_format_from_playground(rdf_format, input)?;
        parse_rdf12(input, None, format)
    } else if proof {
        parse_n3_with_source(input, None, Some(label))
    } else {
        parse_n3(input, None)
    }
}

fn rdf_format_from_playground(format: &str, input: &str) -> Result<RdfFormat> {
    match format.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => Ok(guess_rdf_format(input)),
        "ttl" | "turtle" => Ok(RdfFormat::Turtle),
        "trig" => Ok(RdfFormat::Trig),
        "nt" | "n-triples" | "ntriples" => Ok(RdfFormat::NTriples),
        "nq" | "n-quads" | "nquads" => Ok(RdfFormat::NQuads),
        other => Err(EyeronError::new(format!("unknown RDF format for playground: {}", other))),
    }
}

fn guess_rdf_format(input: &str) -> RdfFormat {
    let trimmed = input.trim_start();
    if trimmed.lines().all(|line| {
        let line = line.trim();
        line.is_empty() || line.starts_with('#') || (line.starts_with('<') && line.ends_with(" .") && line.matches('<').count() >= 4)
    }) {
        return RdfFormat::NTriples;
    }

    if looks_like_trig(trimmed) {
        RdfFormat::Trig
    } else {
        RdfFormat::Turtle
    }
}

fn looks_like_trig(input: &str) -> bool {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut prev_non_ws: Option<char> = None;
    for ch in input.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            continue;
        }
        match ch {
            '{' if depth == 0 => {
                if !matches!(prev_non_ws, Some('=') | Some('>') | Some('<')) {
                    return true;
                }
                depth += 1;
            }
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if !ch.is_whitespace() {
            prev_non_ws = Some(ch);
        }
    }
    false
}
