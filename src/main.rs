use eyeron::error::{EyeronError, Result};
use eyeron::{is_rdf_message_log, parse_n3, parse_n3_with_source, parse_rdf12, parse_rdf_message_log, RdfFormat};
use eyeron::printing::{document_debug, rdf12_json, result_to_string};
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
    rdf12_json: bool,
    rdf12_format: Option<RdfFormat>,
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

    if opt.rdf {
        // RDF compatibility mode is accepted.  Files using VERSION "1.2-messages"
        // are replayed as RDF Message Logs; other files use the N3/Turtle subset parser.
    }
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
        let parsed = if opt.rdf12_json {
            parse_rdf12(text, base, opt.rdf12_format.unwrap_or(RdfFormat::Turtle))
        } else if is_rdf_message_log(text) {
            parse_rdf_message_log(text, base)
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

    if opt.rdf12_json {
        print!("{}", rdf12_json(&merged));
        return Ok(());
    }

    if opt.ast {
        print!("{}", document_debug(&merged));
        return Ok(());
    }

    let reasoner_options = ReasonerOptions { proof: opt.proof, ..ReasonerOptions::default() };
    let result = reason(&merged, &reasoner_options);
    if opt.proof {
        print!("{}", proof_to_n3(&merged.prefixes, &result));
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
            "-t" | "--stream" => opt.stream = true,
            "--builtin" | "--store" | "--store-path" => {
                let flag = args[i].clone();
                i += 1;
                if i >= args.len() { return Err(EyeronError::new(format!("{} requires a value", flag))); }
                eprintln!("warning: {} is accepted for CLI compatibility but not implemented in Eyeron", flag);
            }
            "--stream-messages" => opt.stream_messages = true,
            "--rdf12-json" => opt.rdf12_json = true,
            "--rdf12-format" => {
                i += 1;
                if i >= args.len() { return Err(EyeronError::new("--rdf12-format requires a value")); }
                opt.rdf12_format = RdfFormat::parse(&args[i]);
                if opt.rdf12_format.is_none() {
                    return Err(EyeronError::new(format!("unknown RDF 1.2 format {}", args[i])));
                }
            }
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
    println!("Usage: eyeron [options] [file.n3|- ...]");
    println!();
    println!("Options:");
    println!("  -a, --ast                     Print parsed AST/debug form and exit");
    println!("  -p, --proof                   Enable N3 proof explanations");
    println!("  -r, --rdf                     Enable RDF-compatible input mode; RDF Message Logs are replayed");
    println!("  -t, --stream                  Output is emitted after fixpoint");
    println!("      --stream-messages         RDF Message Log input with VERSION/MESSAGE delimiters");
    println!("      --rdf12-json              Parse RDF 1.2 syntax and emit JSON quads");
    println!("      --rdf12-format FORMAT     RDF 1.2 format: turtle, n-triples, n-quads, or trig");
    println!("      --base-iri IRI            Base IRI used by parser modes that resolve relative IRIs");
    println!("  -v, --version                 Print version");
    println!("  -h, --help                    Show this help");
}
