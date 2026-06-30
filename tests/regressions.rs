use eyeron::{parse_n3, reason, reason_document, result_to_string, ReasonerOptions};

fn check_golden_non_prefix_lines(name: &str, source: &str, golden: &str) -> std::result::Result<(), String> {
    let out = reason(source).map_err(|err| format!("{} failed: {}", name, err))?;
    for expected in stable_golden_lines(golden) {
        if !out.contains(expected) {
            return Err(format!("{} missing golden line `{}`\nactual:\n{}", name, expected, out));
        }
    }
    Ok(())
}

fn assert_golden_non_prefix_lines(name: &str, source: &str, golden: &str) {
    check_golden_non_prefix_lines(name, source, golden).unwrap_or_else(|msg| panic!("{}", msg));
}

fn stable_golden_lines(golden: &str) -> impl Iterator<Item = &str> {
    golden.lines().map(str::trim).filter(|line| {
        !line.is_empty()
            && !line.starts_with("@prefix")
            && !line.starts_with("#")
            && !line.starts_with("- [")
            && !line.contains("_:")
            && !matches!(*line, "{" | "}" | "} .")
    })
}

fn progress_line(message: &str) {
    use std::io::Write;
    let line = format!("{}\n", message);
    #[cfg(unix)]
    {
        if let Ok(mut stderr) = std::fs::OpenOptions::new().write(true).open("/dev/stderr") {
            let _ = stderr.write_all(line.as_bytes());
            let _ = stderr.flush();
            return;
        }
    }
    eprint!("{}", line);
}

#[test]
fn witch_derives_girl_as_witch() {
    assert_golden_non_prefix_lines(
        "witch",
        include_str!("../examples/witch.n3"),
        include_str!("../examples/output/witch.n3"),
    );
}

#[test]
fn equals_surface_syntax_maps_to_owl_same_as() {
    assert_golden_non_prefix_lines(
        "equals",
        include_str!("../examples/equals.n3"),
        include_str!("../examples/output/equals.n3"),
    );
}

#[test]
fn log_query_can_emit_output_string() {
    let out = reason(include_str!("../examples/collection.n3")).unwrap();
    assert!(out.contains("# collection"), "{}", out);
    assert!(out.contains("Source files"), "{}", out);
}

#[test]
fn family_cousins_numeric_generation() {
    let doc = parse_n3(include_str!("../examples/family-cousins.n3"), None).unwrap();
    let result = reason_document(&doc, &ReasonerOptions::default());
    let out = result_to_string(&doc.prefixes, &result.derived);
    assert!(out.contains(":Bob :generation 1"), "{}", out);
    assert!(out.contains(":Dave :generation 2"), "{}", out);
    assert!(out.contains(":Heidi :generation 3"), "{}", out);
    assert!(out.contains(":Heidi :cousin :Judy"), "{}", out);
}

#[test]
fn simple_golden_examples_match_expected_lines() {
    let cases = [
        ("backward", include_str!("../examples/backward.n3"), include_str!("../examples/output/backward.n3")),
        ("schema-foaf-mapping", include_str!("../examples/schema-foaf-mapping.n3"), include_str!("../examples/output/schema-foaf-mapping.n3")),
        ("similar", include_str!("../examples/similar.n3"), include_str!("../examples/output/similar.n3")),
        ("monkey", include_str!("../examples/monkey.n3"), include_str!("../examples/output/monkey.n3")),
        ("rdf-list", include_str!("../examples/rdf-list.n3"), include_str!("../examples/output/rdf-list.n3")),
        ("rule-matching", include_str!("../examples/rule-matching.n3"), include_str!("../examples/output/rule-matching.n3")),
        ("log-not-includes", include_str!("../examples/log-not-includes.n3"), include_str!("../examples/output/log-not-includes.n3")),
    ];
    for (name, source, golden) in cases {
        assert_golden_non_prefix_lines(name, source, golden);
    }
}

#[test]
fn derived_rules_are_promoted_to_active_rules() {
    let out = reason(include_str!("../examples/derived-rule.n3")).unwrap();
    assert!(out.contains("=>"), "{}", out);
    assert!(out.contains(":test :is true"), "{}", out);

    let out = reason(include_str!("../examples/derived-backward-rule.n3")).unwrap();
    assert!(out.contains("<="), "{}", out);
    assert!(out.contains(":bob :hasParent :alice"), "{}", out);
    assert!(!out.contains(":bob :childOf :alice"), "derived backward rules must not materialize the goal fact:\n{}", out);
}

#[test]
fn cat_koko_keeps_generated_rule_blank_scopes_distinct() {
    let out = reason(include_str!("../examples/cat-koko.n3")).unwrap();
    assert!(out.contains("a :Cat"), "{}", out);
    assert!(out.contains("a :BritishShortHair"), "{}", out);
    assert!(out.contains(":test :is true"), "{}", out);
}

#[test]
fn formula_terms_can_be_derived_as_objects() {
    let out = reason(include_str!("../examples/good-cobbler.n3")).unwrap();
    assert!(out.contains(":test :is"), "{}", out);
    assert!(out.contains(":joe :is (:good :Cobbler)"), "{}", out);
}

#[test]
fn existential_rule_still_introduces_distinct_blank_nodes() {
    let out = reason(include_str!("../examples/existential-rule.n3")).unwrap();
    assert!(out.contains(":Socrates :is _:"), "{}", out);
    assert!(out.contains(":Plato :is _:"), "{}", out);
}

#[test]
fn collect_all_and_list_builtins_match_golden_lines() {
    let cases = [
        ("dog", include_str!("../examples/dog.n3"), include_str!("../examples/output/dog.n3")),
        ("log-collect-all-in", include_str!("../examples/log-collect-all-in.n3"), include_str!("../examples/output/log-collect-all-in.n3")),
        ("list-iterate", include_str!("../examples/list-iterate.n3"), include_str!("../examples/output/list-iterate.n3")),
        ("list-map", include_str!("../examples/list-map.n3"), include_str!("../examples/output/list-map.n3")),
    ];
    for (name, source, golden) in cases {
        assert_golden_non_prefix_lines(name, source, golden);
    }
}

#[test]
fn all_packaged_example_goldens_match_expected_lines() {
    use std::fs;
    use std::path::Path;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output_dir = root.join("examples").join("output");
    let mut by_name: std::collections::BTreeMap<String, std::path::PathBuf> = std::collections::BTreeMap::new();

    for entry in fs::read_dir(&output_dir).expect("examples/output directory exists") {
        let entry = entry.expect("read examples/output entry");
        let path = entry.path();
        let ext = path.extension().and_then(|ext| ext.to_str());
        if !matches!(ext, Some("n3") | Some("md")) {
            continue;
        }

        let name = path.file_stem().and_then(|stem| stem.to_str()).expect("utf8 example name").to_string();
        let source_path = root.join("examples").join(format!("{}.n3", name));
        if !source_path.exists() {
            continue;
        }

        match by_name.get(&name) {
            Some(existing) => {
                let existing_ext = existing.extension().and_then(|ext| ext.to_str());
                // Prefer .md goldens over stale .n3 goldens when both are present.
                // This makes the test robust when users unpack a new release over an older tree.
                if existing_ext == Some("n3") && ext == Some("md") {
                    by_name.insert(name, path);
                }
            }
            None => {
                by_name.insert(name, path);
            }
        }
    }

    let mut cases: Vec<_> = by_name
        .into_iter()
        .map(|(name, golden_path)| {
            let source_path = root.join("examples").join(format!("{}.n3", name));
            (name, source_path, golden_path)
        })
        .collect();

    cases.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!cases.is_empty(), "no example/golden pairs found");

    // Keep known heavy/unsupported examples out of the default cargo test sweep.
    // They remain packaged as examples, but need targeted scheduler work before
    // they can be part of the always-on regression suite.
    const SKIPPED_EXAMPLES: &[&str] = &["dining-philosophers"];

    for (name, source_path, golden_path) in cases {
        if SKIPPED_EXAMPLES.contains(&name.as_str()) {
            progress_line(&format!(
                "skipping examples/{}.n3 (known scheduler/performance TODO)",
                name
            ));
            continue;
        }

        progress_line(&format!("checking examples/{}.n3", name));
        let started = std::time::Instant::now();

        let source = fs::read_to_string(&source_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {}", source_path.display(), err));
        let golden = fs::read_to_string(&golden_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {}", golden_path.display(), err));

        let (tx, rx) = std::sync::mpsc::channel();
        let thread_name = name.clone();
        std::thread::spawn(move || {
            let result = check_golden_non_prefix_lines(&thread_name, &source, &golden);
            let _ = tx.send(result);
        });

        let timeout = if name.starts_with("deep-taxonomy-") {
            std::time::Duration::from_secs(60)
        } else {
            std::time::Duration::from_secs(20)
        };

        match rx.recv_timeout(timeout) {
            Ok(Ok(())) => progress_line(&format!(
                "ok examples/{}.n3 ({:.3}s)",
                name,
                started.elapsed().as_secs_f64()
            )),
            Ok(Err(msg)) => panic!("{}", msg),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!(
                    "{} exceeded the {:.0}s per-example golden-test limit after {:.3}s",
                    name,
                    timeout.as_secs_f64(),
                    started.elapsed().as_secs_f64()
                );
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("{} golden-test worker terminated without reporting a result", name);
            }
        }
    }
}

