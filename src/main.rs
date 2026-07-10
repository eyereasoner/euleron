use eyeron::error::{EyeronError, Result};
use eyeron::{is_rdf_message_log, parse_n3, parse_n3_with_source, parse_rdf12, parse_rdf_message_log, RdfFormat};
use eyeron::printing::{document_debug, rdf_result_to_string, result_to_string};
use eyeron::proof::proof_to_n3;
use eyeron::reasoner::{reason, ReasonerOptions};
use eyeron::Document;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Default)]
struct CliOptions {
    ast: bool,
    proof: bool,
    rdf: bool,
    stream: bool,
    stream_messages: bool,
    base_iri: Option<String>,
    files: Vec<String>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("eyeron: {}", err);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let opt = parse_args(env::args().skip(1).collect())?;

    if opt.stream {
        eprintln!("warning: --stream is accepted; Eyeron currently emits after the fixpoint is reached");
    }
    if opt.stream_messages {
        // RDF Message Logs are auto-detected by VERSION/MESSAGE delimiters.
    }
    let sources = read_sources(&opt.files)?;
    let mut merged = Document::new();
    for (label, text) in &sources {
        let path_base = if label == "<stdin>" { None } else { path_to_file_iri(label).ok() };
        let base = opt.base_iri.as_deref().or(path_base.as_deref());
        let parsed = if is_rdf_message_log(text) {
            parse_rdf_message_log(text, base)
        } else if let Some(format) = rdf_format_for_source(label, opt.rdf)? {
            parse_rdf12(text, base, format)
        } else if opt.proof {
            parse_n3_with_source(text, base, Some(label))
        } else {
            parse_n3(text, base)
        };
        match parsed {
            Ok(doc) => merged.merge(doc),
            Err(err) => return Err(EyeronError::new(err.with_source_location(text, label))),
        }
    }

    if opt.ast {
        print!("{}", document_debug(&merged));
        return Ok(());
    }

    let reasoner_options = ReasonerOptions { proof: opt.proof, ..ReasonerOptions::default() };
    let result = reason(&merged, &reasoner_options);
    if opt.proof {
        print!("{}", proof_to_n3(&merged.prefixes, &result));
    } else if opt.rdf {
        print!("{}", rdf_result_to_string(&merged.prefixes, &result.derived));
    } else {
        print!("{}", result_to_string(&merged.prefixes, &result.derived));
    }
    Ok(())
}

fn parse_args(args: Vec<String>) -> Result<CliOptions> {
    let mut opt = CliOptions::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "-v" | "--version" => {
                println!("{}", VERSION);
                std::process::exit(0);
            }
            "-a" | "--ast" => opt.ast = true,
            "-p" | "--proof" | "--proof-comments" => opt.proof = true,
            "-r" | "--rdf" => opt.rdf = true,
            "-s" | "--stream" => opt.stream = true,
            "--builtin" | "--store" | "--store-path" => {
                let flag = args[i].clone();
                i += 1;
                if i >= args.len() { return Err(EyeronError::new(format!("{} requires a value", flag))); }
                eprintln!("warning: {} is accepted for CLI compatibility but not implemented in Eyeron", flag);
            }
            "--stream-messages" => opt.stream_messages = true,
            "--base-iri" | "--base" => {
                i += 1;
                if i >= args.len() { return Err(EyeronError::new(format!("{} requires a value", args[i - 1]))); }
                opt.base_iri = Some(args[i].clone());
            }
            "--store-clear" | "--enforce-https" => {
                eprintln!("warning: {} is accepted for CLI compatibility but not implemented in Eyeron", args[i]);
            }
            other if other.starts_with('-') && other != "-" => {
                return Err(EyeronError::new(format!("unknown option {}", other)));
            }
            file => opt.files.push(file.to_string()),
        }
        i += 1;
    }
    Ok(opt)
}

fn rdf_format_for_source(label: &str, rdf_mode: bool) -> Result<Option<RdfFormat>> {
    if label == "<stdin>" {
        return Ok(if rdf_mode { Some(RdfFormat::Turtle) } else { None });
    }

    let extension = Path::new(label)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);
    let format = extension.as_deref().and_then(RdfFormat::parse);

    match (format, extension.as_deref(), rdf_mode) {
        (Some(format), _, _) => Ok(Some(format)),
        (None, Some("n3"), _) => Ok(None),
        (None, _, true) => Err(EyeronError::new(format!(
            "cannot infer RDF format for {}; use .ttl, .nt, .nq, .trig, or .n3",
            label
        ))),
        (None, _, false) => Ok(None),
    }
}

fn read_sources(files: &[String]) -> Result<Vec<(String, String)>> {
    if files.is_empty() {
        let mut s = String::new();
        io::stdin().read_to_string(&mut s)?;
        return Ok(vec![("<stdin>".to_string(), s)]);
    }

    let mut out = Vec::new();
    for f in files {
        if f == "-" {
            let mut s = String::new();
            io::stdin().read_to_string(&mut s)?;
            out.push(("<stdin>".to_string(), s));
        } else {
            out.push((f.clone(), fs::read_to_string(f)?));
        }
    }
    Ok(out)
}

fn path_to_file_iri(path: &str) -> std::result::Result<String, ()> {
    let abs = fs::canonicalize(Path::new(path)).map_err(|_| ())?;
    let s = abs.to_string_lossy().replace('\\', "/");
    Ok(format!("file://{}{}", if s.starts_with('/') { "" } else { "/" }, percent_encode_path(&s)))
}

fn percent_encode_path(path: &str) -> String {
    let mut out = String::new();
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => out.push(char::from(b)),
            other => out.push_str(&format!("%{:02X}", other)),
        }
    }
    out
}

fn print_help() {
    println!("eyeron {}", VERSION);
    println!();
    println!("Usage: eyeron [options] [file.n3|file.ttl|file.nt|file.nq|file.trig|- ...]");
    println!();
    println!("Options:");
    println!("  -a, --ast                     Print parsed AST/debug form and exit");
    println!("  -p, --proof                   Enable N3 proof explanations");
    println!("  -r, --rdf                     Enable RDF/TriG input/output compatibility");
    println!("  -s, --stream                  Output is emitted after fixpoint");
    println!("      --stream-messages         RDF Message Log input with VERSION/MESSAGE delimiters");
    println!("      --base-iri IRI            Base IRI used by parser modes that resolve relative IRIs");
    println!("  -v, --version                 Print version");
    println!("  -h, --help                    Show this help");
}
