use eyeron::error::{EyeronError, Result};
use eyeron::parser::parse_n3;
use eyeron::printing::{document_debug, result_to_string};
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
    super_restricted: bool,
    deterministic_skolem: bool,
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

    if opt.proof {
        eprintln!("warning: --proof is accepted but proof comments are not implemented in Eyeron yet");
    }
    if opt.rdf {
        eprintln!("warning: --rdf compatibility mode is accepted but currently parsed as N3/Turtle subset");
    }
    if opt.stream {
        eprintln!("warning: --stream is accepted; Eyeron currently emits after the fixpoint is reached");
    }
    if opt.super_restricted || opt.deterministic_skolem {
        // These flags are currently no-ops, but accepted to ease migration from the JS CLI.
    }

    let sources = read_sources(&opt.files)?;
    let mut merged = Document::new();
    for (label, text) in &sources {
        let base = if label == "<stdin>" { None } else { path_to_file_iri(label).ok() };
        match parse_n3(text, base.as_deref()) {
            Ok(doc) => merged.merge(doc),
            Err(err) => return Err(EyeronError::new(err.with_source_location(text, label))),
        }
    }

    if opt.ast {
        print!("{}", document_debug(&merged));
        return Ok(());
    }

    let result = reason(&merged, &ReasonerOptions::default());
    print!("{}", result_to_string(&merged.prefixes, &result.derived));
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
            "-p" | "--proof" => opt.proof = true,
            "-r" | "--rdf" => opt.rdf = true,
            "-t" | "--stream" => opt.stream = true,
            "-s" | "--super-restricted" => opt.super_restricted = true,
            "-d" | "--deterministic-skolem" => opt.deterministic_skolem = true,
            "--builtin" | "--store" | "--store-path" => {
                let flag = args[i].clone();
                i += 1;
                if i >= args.len() { return Err(EyeronError::new(format!("{} requires a value", flag))); }
                eprintln!("warning: {} is accepted for CLI compatibility but not implemented in Eyeron", flag);
            }
            "--store-clear" | "--stream-messages" | "--enforce-https" => {
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
    println!("  -p, --proof                   Accepted; proof comments not implemented yet");
    println!("  -r, --rdf                     Accepted; parsed as N3/Turtle subset");
    println!("  -t, --stream                  Accepted; output is emitted after fixpoint");
    println!("  -s, --super-restricted        Accepted for compatibility");
    println!("  -d, --deterministic-skolem    Accepted for compatibility");
    println!("  -v, --version                 Print version");
    println!("  -h, --help                    Show this help");
}
