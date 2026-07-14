use eyeron::error::{EyeronError, Result};
use eyeron::printing::{document_debug, rdf_result_to_string, result_to_string};
use eyeron::proof::proof_to_n3;
use eyeron::reasoner::{reason, ReasonerOptions};
use eyeron::Document;
use eyeron::{
    is_rdf_message_log, parse_n3, parse_n3_with_source, parse_rdf12, parse_rdf_message_log,
    RdfFormat,
};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
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
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        return Ok(());
    }
    let opt = parse_args(args)?;

    if opt.stream {
        eprintln!(
            "warning: --stream is accepted; Eyeron currently emits after the fixpoint is reached"
        );
    }
    if opt.stream_messages {
        return run_stream_messages(&opt);
    }
    let sources = read_sources(&opt.files)?;
    let mut merged = Document::new();
    for (label, text) in &sources {
        let path_base = if label == "<stdin>" {
            None
        } else if is_http_url(label) {
            Some(label.clone())
        } else {
            path_to_file_iri(label).ok()
        };
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

    let reasoner_options = ReasonerOptions {
        proof: opt.proof,
        ..ReasonerOptions::default()
    };
    let result = reason(&merged, &reasoner_options);
    if let Some(summary) = result.incomplete_summary() {
        return Err(EyeronError::new(summary));
    }
    if opt.proof {
        print!("{}", proof_to_n3(&merged.prefixes, &result));
    } else if opt.rdf {
        print!(
            "{}",
            rdf_result_to_string(&merged.prefixes, &result.derived)
        );
    } else {
        print!("{}", result_to_string(&merged.prefixes, &result.derived));
    }
    Ok(())
}

fn run_stream_messages(opt: &CliOptions) -> Result<()> {
    if !opt.rdf {
        return Err(EyeronError::new("--stream-messages requires -r/--rdf"));
    }
    if opt.ast || opt.proof || opt.stream {
        return Err(EyeronError::new(
            "--stream-messages cannot be combined with --ast, --proof, or --stream",
        ));
    }

    let mut program = Document::new();
    let mut message_sources = Vec::new();
    for source in &opt.files {
        if is_http_url(source) {
            message_sources.push(source.clone());
            continue;
        }
        let text = if source == "-" {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text)?;
            text
        } else {
            fs::read_to_string(source)?
        };
        if is_rdf_message_log(&text) {
            message_sources.push(source.clone());
        } else {
            let base = opt.base_iri.as_deref().or_else(|| None);
            let parsed = parse_n3_with_source(&text, base, Some(source))
                .map_err(|err| EyeronError::new(err.with_source_location(&text, source)))?;
            program.merge(parsed);
        }
    }
    if message_sources.is_empty() {
        return Err(EyeronError::new(
            "--stream-messages did not find any RDF Message Log input",
        ));
    }

    for source in message_sources {
        let base = opt.base_iri.clone().or_else(|| {
            if is_http_url(&source) {
                Some(source.clone())
            } else {
                path_to_file_iri(&source).ok()
            }
        });
        if is_http_url(&source) {
            let response = ureq::get(&source)
                .call()
                .map_err(|err| EyeronError::new(format!("failed to fetch {source}: {err}")))?;
            let final_url = response.get_url().to_string();
            stream_message_reader(
                BufReader::new(response.into_reader()),
                &final_url,
                base.as_deref(),
                &program,
            )?;
        } else if source == "-" {
            return Err(EyeronError::new(
                "stdin RDF Message Logs cannot follow another stdin read",
            ));
        } else {
            let file = fs::File::open(&source)?;
            stream_message_reader(
                BufReader::new(file),
                &source,
                base.as_deref(),
                &program,
            )?;
        }
    }
    Ok(())
}

fn stream_message_reader<R: BufRead>(
    mut reader: R,
    label: &str,
    base: Option<&str>,
    program: &Document,
) -> Result<()> {
    let mut directives = String::new();
    let mut message = String::new();
    let mut line = String::new();
    let mut saw_version = false;
    let mut saw_delimiter = false;
    let mut message_index = 1usize;

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).map_err(|err| {
            EyeronError::new(format!("failed to read response from {label}: {err}"))
        })?;
        if bytes == 0 {
            break;
        }
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if (lower.starts_with("version ") || lower.starts_with("@version "))
            && lower.contains("-messages")
        {
            saw_version = true;
        } else if trimmed.eq_ignore_ascii_case("MESSAGE")
            || trimmed.eq_ignore_ascii_case("@message .")
        {
            run_one_message(
                program,
                &directives,
                &message,
                label,
                message_index,
                base,
            )?;
            message.clear();
            message_index += 1;
            saw_delimiter = true;
        } else if trimmed.starts_with("PREFIX ")
            || trimmed.starts_with("prefix ")
            || trimmed.starts_with("BASE ")
            || trimmed.starts_with("base ")
        {
            if !directives.contains(&line) {
                directives.push_str(&line);
            }
        } else {
            message.push_str(&line);
        }
    }
    if !saw_version {
        return Err(EyeronError::new(format!(
            "not an RDF Message Log: missing VERSION \"*-messages\" directive in {label}"
        )));
    }
    if saw_delimiter || !message.trim().is_empty() {
        run_one_message(
            program,
            &directives,
            &message,
            label,
            message_index,
            base,
        )?;
    }
    Ok(())
}

fn run_one_message(
    program: &Document,
    directives: &str,
    message: &str,
    label: &str,
    index: usize,
    base: Option<&str>,
) -> Result<()> {
    let replay = format!("{directives}\nVERSION \"1.2-messages\"\n{message}");
    let message_label = format!("{label}#message-{index}");
    let mut merged = program.clone();
    let parsed = parse_rdf_message_log(&replay, base)
        .map_err(|err| EyeronError::new(err.with_source_location(&replay, &message_label)))?;
    merged.merge(parsed);
    let options = ReasonerOptions {
        proof: false,
        ..ReasonerOptions::default()
    };
    let result = reason(&merged, &options);
    if let Some(summary) = result.incomplete_summary() {
        return Err(EyeronError::new(summary));
    }
    print!(
        "{}",
        rdf_result_to_string(&BTreeMap::new(), &result.derived)
    );
    io::stdout().flush()?;
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
                if i >= args.len() {
                    return Err(EyeronError::new(format!("{} requires a value", flag)));
                }
                eprintln!(
                    "warning: {} is accepted for CLI compatibility but not implemented in Eyeron",
                    flag
                );
            }
            "--stream-messages" => opt.stream_messages = true,
            "--base-iri" | "--base" => {
                i += 1;
                if i >= args.len() {
                    return Err(EyeronError::new(format!(
                        "{} requires a value",
                        args[i - 1]
                    )));
                }
                opt.base_iri = Some(args[i].clone());
            }
            "--store-clear" | "--enforce-https" => {
                eprintln!(
                    "warning: {} is accepted for CLI compatibility but not implemented in Eyeron",
                    args[i]
                );
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
        return Ok(if rdf_mode {
            Some(RdfFormat::Turtle)
        } else {
            None
        });
    }

    let source_path = label.split(['?', '#']).next().unwrap_or(label);
    let extension = Path::new(source_path)
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
        } else if is_http_url(f) {
            let response = ureq::get(f)
                .call()
                .map_err(|err| EyeronError::new(format!("failed to fetch {f}: {err}")))?;
            let final_url = response.get_url().to_string();
            let text = response.into_string().map_err(|err| {
                EyeronError::new(format!("failed to read response from {f}: {err}"))
            })?;
            out.push((final_url, text));
        } else {
            out.push((f.clone(), fs::read_to_string(f)?));
        }
    }
    Ok(out)
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn path_to_file_iri(path: &str) -> std::result::Result<String, ()> {
    let abs = fs::canonicalize(Path::new(path)).map_err(|_| ())?;
    let s = abs.to_string_lossy().replace('\\', "/");
    Ok(format!(
        "file://{}{}",
        if s.starts_with('/') { "" } else { "/" },
        percent_encode_path(&s)
    ))
}

fn percent_encode_path(path: &str) -> String {
    let mut out = String::new();
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b))
            }
            other => out.push_str(&format!("%{:02X}", other)),
        }
    }
    out
}

fn print_help() {
    println!("eyeron {}", VERSION);
    println!();
    println!("Usage: eyeron [options] [file-or-url|- ...]");
    println!();
    println!("Options:");
    println!("  -a, --ast                     Print parsed AST/debug form and exit");
    println!("  -p, --proof                   Enable N3 proof explanations");
    println!("  -r, --rdf                     Enable RDF/TriG input/output compatibility");
    println!("  -s, --stream                  Output is emitted after fixpoint");
    println!(
        "      --stream-messages         RDF Message Log input with VERSION/MESSAGE delimiters"
    );
    println!(
        "      --base-iri IRI            Base IRI used by parser modes that resolve relative IRIs"
    );
    println!("  -v, --version                 Print version");
    println!("  -h, --help                    Show this help");
}
