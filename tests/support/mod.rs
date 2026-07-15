use eyeron::{
    is_rdf_message_log, parse_n3, parse_rdf_message_log, reason_document, result_to_string,
    Document, ReasonerOptions,
};

pub fn check_golden_documents(
    name: &str,
    sources: Vec<(&str, &str)>,
    golden: &str,
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
    for expected in stable_golden_lines(golden) {
        if !out.contains(expected) {
            return Err(format!(
                "{name} missing golden line `{expected}`\nactual:\n{out}"
            ));
        }
    }
    Ok(())
}

fn stable_golden_lines(golden: &str) -> impl Iterator<Item = &str> {
    golden.lines().map(str::trim).filter(|line| {
        !line.is_empty()
            && !line.starts_with("@prefix")
            && !line.starts_with('#')
            && !line.starts_with("- [")
            && !line.contains("_:")
            && !matches!(*line, "{" | "}" | "} .")
    })
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

    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }

    match std::env::var("CARGO_TERM_COLOR").as_deref() {
        Ok("always") => true,
        Ok("never") => false,
        _ => std::io::stderr().is_terminal(),
    }
}

fn colour(text: &str, ansi_code: u8) -> String {
    if colour_enabled() {
        format!("\x1b[{ansi_code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn green(text: &str) -> String {
    colour(text, 32)
}

pub fn red(text: &str) -> String {
    colour(text, 31)
}
