mod support;

use eyeron::{parse_n3, parse_n3_with_source, proof_to_n3, reason_document, ReasonerOptions};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use support::{check_golden_documents, green, progress_line, red};

fn main() {
    let started = std::time::Instant::now();
    proof_goldens_are_valid_n3_documents();
    every_proof_golden_has_a_source_that_generates_a_valid_proof();
    selected_proof_examples_match_eyeling_style_goldens();
    every_top_level_n3_example_parses();
    let count = all_packaged_example_goldens_match_expected_lines();

    progress_line(&format!(
        "\nexample result: {}. {count} passed; 0 failed; finished in {:.2}s",
        green("ok"),
        started.elapsed().as_secs_f64()
    ));
}

fn proof_goldens_are_valid_n3_documents() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let proof_dir = root.join("examples/proof");
    assert!(proof_dir.exists(), "examples/proof directory is missing");

    let files = sorted_n3_files(&proof_dir, "examples/proof");
    assert!(
        !files.is_empty(),
        "no proof goldens found in examples/proof"
    );

    for path in files {
        let source = read(&path);
        parse_n3(&source, None).unwrap_or_else(|err| {
            panic!(
                "proof golden {} is not parseable N3: {}",
                path.display(),
                err
            )
        });
    }
}

fn every_proof_golden_has_a_source_that_generates_a_valid_proof() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let proof_dir = root.join("examples/proof");

    for golden_path in sorted_n3_files(&proof_dir, "examples/proof") {
        let name = golden_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("utf8 proof name");
        let source_path = root.join("examples").join(name);
        assert!(
            source_path.exists(),
            "{} has no corresponding source example",
            golden_path.display()
        );
        let source = read(&source_path);
        let label = source_path.to_string_lossy();
        let doc = parse_n3_with_source(&source, None, Some(label.as_ref()))
            .unwrap_or_else(|err| panic!("failed to parse {}: {}", source_path.display(), err));
        let result = reason_document(
            &doc,
            &ReasonerOptions {
                proof: true,
                ..ReasonerOptions::default()
            },
        );
        let proof = proof_to_n3(&doc.prefixes, &result);
        assert!(
            !proof.trim().is_empty(),
            "{} generated an empty proof",
            source_path.display()
        );
        parse_n3(&proof, None).unwrap_or_else(|err| {
            panic!(
                "generated proof for {} is not valid N3: {}\n{}",
                name, err, proof
            )
        });
    }
}

fn selected_proof_examples_match_eyeling_style_goldens() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    for name in ["backward", "socrates"] {
        let source_path = root.join("examples").join(format!("{name}.n3"));
        let golden_path = root.join("examples/proof").join(format!("{name}.n3"));
        let source = read(&source_path);
        let golden = read(&golden_path);
        let label = source_path.to_string_lossy();
        let doc = parse_n3_with_source(&source, None, Some(label.as_ref()))
            .unwrap_or_else(|err| panic!("failed to parse {}: {}", source_path.display(), err));
        let result = reason_document(
            &doc,
            &ReasonerOptions {
                proof: true,
                ..ReasonerOptions::default()
            },
        );
        let out = proof_to_n3(&doc.prefixes, &result);

        assert_eq!(
            normalize_proof_golden(&golden),
            normalize_proof_golden(&out),
            "proof example {name} did not match {}\nactual:\n{}",
            golden_path.display(),
            out
        );
    }
}

fn every_top_level_n3_example_parses() {
    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let files = sorted_n3_files(&examples_dir, "examples");
    assert!(!files.is_empty(), "no top-level N3 examples found");

    for path in files {
        let source = read(&path);
        let label = path.to_string_lossy();
        parse_n3_with_source(&source, None, Some(label.as_ref()))
            .unwrap_or_else(|err| panic!("example {} is not valid N3: {}", path.display(), err));
    }
}

fn all_packaged_example_goldens_match_expected_lines() -> usize {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output_dir = root.join("examples/output");
    let mut by_name: BTreeMap<String, PathBuf> = BTreeMap::new();

    for entry in fs::read_dir(&output_dir).expect("examples/output directory exists") {
        let path = entry.expect("read examples/output entry").path();
        let ext = path.extension().and_then(|ext| ext.to_str());
        if !matches!(ext, Some("n3") | Some("md")) {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("utf8 example name")
            .to_string();
        let source_path = root.join("examples").join(format!("{name}.n3"));
        if !source_path.exists() {
            continue;
        }

        match by_name.get(&name) {
            Some(existing)
                if existing.extension().and_then(|ext| ext.to_str()) == Some("n3")
                    && ext == Some("md") =>
            {
                // Prefer .md goldens when both formats are present.
                by_name.insert(name, path);
            }
            None => {
                by_name.insert(name, path);
            }
            _ => {}
        }
    }

    let cases = by_name
        .into_iter()
        .map(|(name, golden_path)| {
            let source_path = root.join("examples").join(format!("{name}.n3"));
            (name, source_path, golden_path)
        })
        .collect::<Vec<_>>();
    assert!(!cases.is_empty(), "no example/golden pairs found");

    let count = cases.len();
    progress_line(&format!(
        "running {count} example{}",
        if count == 1 { "" } else { "s" }
    ));

    for (name, source_path, golden_path) in cases {
        run_golden_case(root, name, source_path, golden_path);
    }

    count
}

fn run_golden_case(root: &Path, name: String, source_path: PathBuf, golden_path: PathBuf) {
    let started = std::time::Instant::now();
    let source = read(&source_path);
    let input_path = root.join("examples/input").join(format!("{name}.trig"));
    let input = input_path.exists().then(|| read(&input_path));
    let golden = read(&golden_path);
    let golden_is_n3 = golden_path.extension().and_then(|ext| ext.to_str()) == Some("n3");

    let (tx, rx) = std::sync::mpsc::channel();
    let thread_name = name.clone();
    std::thread::spawn(move || {
        let mut sources = vec![("rules", source.as_str())];
        if let Some(input) = input.as_ref() {
            sources.push(("input", input.as_str()));
        }
        let _ = tx.send(check_golden_documents(&thread_name, sources, &golden, golden_is_n3));
    });

    let timeout = if name.starts_with("deep-taxonomy-")
        || name.starts_with("rdf-message-")
        || name == "dining-philosophers"
    {
        std::time::Duration::from_secs(60)
    } else {
        std::time::Duration::from_secs(20)
    };

    match rx.recv_timeout(timeout) {
        Ok(Ok(())) => report_case(&name, &green("ok"), started),
        Ok(Err(msg)) => {
            report_case(&name, &red("fail"), started);
            panic!("{msg}");
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            report_case(&name, &red("fail"), started);
            panic!(
                "{name} exceeded the {:.0}s per-example golden-test limit after {:.3}s",
                timeout.as_secs_f64(),
                started.elapsed().as_secs_f64()
            );
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            report_case(&name, &red("fail"), started);
            panic!("{name} golden-test worker terminated without reporting a result");
        }
    }
}

fn report_case(name: &str, status: &str, started: std::time::Instant) {
    progress_line(&format!(
        "example examples/{name}.n3 ... {status} ({:.3}s)",
        started.elapsed().as_secs_f64()
    ));
}

fn sorted_n3_files(directory: &Path, label: &str) -> Vec<PathBuf> {
    let mut files = fs::read_dir(directory)
        .unwrap_or_else(|err| panic!("failed to read {label}: {err}"))
        .map(|entry| {
            entry
                .unwrap_or_else(|err| panic!("failed to read {label} entry: {err}"))
                .path()
        })
        .filter(|path| {
            path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("n3")
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {}", path.display(), err))
}

fn normalize_proof_golden(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_string()
}
