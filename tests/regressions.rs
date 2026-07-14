use eyeron::{is_rdf_message_log, parse_n3, parse_n3_with_source, parse_rdf12, parse_rdf_message_log, proof_to_n3, rdf_result_to_string, reason, reason_document, result_to_string, Document, RdfFormat, ReasonerOptions};

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

fn check_golden_documents(name: &str, sources: Vec<(&str, &str)>, golden: &str) -> std::result::Result<(), String> {
    let mut doc = Document::new();
    for (label, source) in sources {
        let parsed = if is_rdf_message_log(source) {
            parse_rdf_message_log(source, None)
        } else {
            parse_n3(source, None)
        }.map_err(|err| format!("{} failed to parse {}: {}", name, label, err))?;
        doc.merge(parsed);
    }
    let result = reason_document(&doc, &ReasonerOptions::default());
    let out = result_to_string(&doc.prefixes, &result.derived);
    for expected in stable_golden_lines(golden) {
        if !out.contains(expected) {
            return Err(format!("{} missing golden line `{}`\nactual:\n{}", name, expected, out));
        }
    }
    Ok(())
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
        format!("\x1b[{}m{}\x1b[0m", ansi_code, text)
    } else {
        text.to_string()
    }
}

fn green(text: &str) -> String {
    colour(text, 32)
}

fn red(text: &str) -> String {
    colour(text, 31)
}


#[test]
fn rdf_trig_query_selects_dataset_without_rule_feedback() {
    let trig = r#"
        PREFIX : <http://example.org/#>

        :g { :s :p :o }
    "#;
    let query = r#"
        @prefix log: <http://www.w3.org/2000/10/swap/log#>.
        PREFIX : <http://example.org/#>

        {?S ?P ?O} log:query {?S ?P ?O}.
    "#;

    let mut doc = Document::new();
    doc.merge(parse_rdf12(trig, None, RdfFormat::Trig).unwrap());
    doc.merge(parse_n3(query, None).unwrap());

    let result = reason_document(&doc, &ReasonerOptions::default());
    let out = rdf_result_to_string(&doc.prefixes, &result.derived);

    assert!(out.contains(":g {"), "{}", out);
    assert!(out.contains("    :s :p :o ."), "{}", out);
    assert!(!out.contains("log:nameOf"), "{}", out);
    assert!(!out.contains("=>"), "{}", out);
}

#[test]
fn rdf12_annotations_share_n3_lexer_parser_profile() {
    let input = r#"
        PREFIX : <http://example.org/>
        :s :p :o {| :source :sensor |} .
    "#;
    let doc = parse_rdf12(input, Some("http://example.org/base"), RdfFormat::Turtle).unwrap();
    let reifies = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
    assert!(doc.facts.iter().any(|t| matches!(&t.p, eyeron::Term::Iri(p) if p == reifies)), "{:#?}", doc.facts);
    assert!(doc.facts.iter().any(|t| matches!(&t.p, eyeron::Term::Iri(p) if p == "http://example.org/source")), "{:#?}", doc.facts);
}

#[test]
fn rdf12_parenthesized_triple_terms_remain_terms() {
    let input = r#"
        PREFIX : <http://example.org/>
        :s :p <<(:a :b :c)>> .
    "#;
    let doc = parse_rdf12(input, Some("http://example.org/base"), RdfFormat::Turtle).unwrap();
    assert!(doc.facts.iter().any(|t| matches!(&t.o, eyeron::Term::Formula(inner) if inner.len() == 1)), "{:#?}", doc.facts);
}


#[test]
fn proof_goldens_are_valid_n3_documents() {
    use std::fs;
    use std::path::Path;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let proof_dir = root.join("examples").join("proof");
    assert!(proof_dir.exists(), "examples/proof directory is missing");

    let mut files = fs::read_dir(&proof_dir)
        .expect("read examples/proof directory")
        .map(|entry| entry.expect("read examples/proof entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("n3"))
        .collect::<Vec<_>>();
    files.sort();
    assert!(!files.is_empty(), "no proof goldens found in examples/proof");

    for path in files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {}", path.display(), err));
        parse_n3(&source, None)
            .unwrap_or_else(|err| panic!("proof golden {} is not parseable N3: {}", path.display(), err));
    }
}

#[test]
fn every_proof_golden_has_a_source_that_generates_a_valid_proof() {
    use std::fs;
    use std::path::Path;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let proof_dir = root.join("examples/proof");
    let mut files = fs::read_dir(&proof_dir)
        .expect("read examples/proof directory")
        .map(|entry| entry.expect("read examples/proof entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("n3"))
        .collect::<Vec<_>>();
    files.sort();

    for golden_path in files {
        let name = golden_path.file_name().and_then(|name| name.to_str()).expect("utf8 proof name");
        let source_path = root.join("examples").join(name);
        assert!(source_path.exists(), "{} has no corresponding source example", golden_path.display());
        let source = fs::read_to_string(&source_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {}", source_path.display(), err));
        let label = source_path.to_string_lossy();
        let doc = parse_n3_with_source(&source, None, Some(label.as_ref()))
            .unwrap_or_else(|err| panic!("failed to parse {}: {}", source_path.display(), err));
        let result = reason_document(&doc, &ReasonerOptions { proof: true, ..ReasonerOptions::default() });
        let proof = proof_to_n3(&doc.prefixes, &result);
        assert!(!proof.trim().is_empty(), "{} generated an empty proof", source_path.display());
        parse_n3(&proof, None)
            .unwrap_or_else(|err| panic!("generated proof for {} is not valid N3: {}\n{}", name, err, proof));
    }
}

#[test]
fn selected_proof_examples_match_eyeling_style_goldens() {
    use std::fs;
    use std::path::Path;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cases = ["backward", "socrates"];

    for name in cases {
        let source_path = root.join("examples").join(format!("{name}.n3"));
        let golden_path = root.join("examples").join("proof").join(format!("{name}.n3"));
        let source = fs::read_to_string(&source_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {}", source_path.display(), err));
        let golden = fs::read_to_string(&golden_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {}", golden_path.display(), err));
        let label = source_path.to_string_lossy();
        let doc = parse_n3_with_source(&source, None, Some(label.as_ref()))
            .unwrap_or_else(|err| panic!("failed to parse {}: {}", source_path.display(), err));
        let result = reason_document(&doc, &ReasonerOptions { proof: true, ..ReasonerOptions::default() });
        let out = proof_to_n3(&doc.prefixes, &result);

        assert_eq!(
            normalize_proof_golden(&golden),
            normalize_proof_golden(&out),
            "proof example {name} did not match {}
actual:
{}",
            golden_path.display(),
            out
        );
    }
}

fn normalize_proof_golden(text: &str) -> String {
    text.replace("\r\n", "\n").trim().to_string()
}

#[test]
fn age_example_supports_current_time_date_difference_and_duration_comparison() {
    use std::fs;
    use std::path::Path;

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/age.n3");
    let source = fs::read_to_string(&path).expect("read examples/age.n3");
    let doc = parse_n3_with_source(&source, None, Some("age.n3")).expect("parse age example");
    let result = reason_document(&doc, &ReasonerOptions { proof: true, ..ReasonerOptions::default() });

    assert!(result.derived.iter().any(|triple| {
        triple.s == eyeron::Term::Iri("https://example.org/#test".to_string())
            && triple.p == eyeron::Term::Iri("https://example.org/#is".to_string())
    }));
    let proof = proof_to_n3(&doc.prefixes, &result);
    assert!(proof.contains("pe:builtin time:localTime"), "{proof}");
    assert!(proof.contains("pe:builtin math:difference"), "{proof}");
    assert!(proof.contains("pe:builtin math:greaterThan"), "{proof}");
}


#[test]
fn playground_html_is_packaged_for_browser_wasm() {
    use std::fs;
    use std::path::Path;

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let playground = root.join("playground.html");
    let html = fs::read_to_string(&playground)
        .unwrap_or_else(|err| panic!("failed to read {}: {}", playground.display(), err));

    assert!(html.contains("The Eyeron N3 Playground"), "{}", playground.display());
    assert!(html.contains("./pkg/eyeron.js"), "playground should load the wasm-pack web bundle");
    assert!(html.contains("reasonWithData"), "playground should expose separate data + N3 program reasoning");

    let examples_dir = root.join("examples");
    let mut expected = fs::read_dir(&examples_dir)
        .expect("read examples directory")
        .map(|entry| entry.expect("read examples entry").path())
        .filter(|path| path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("n3"))
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    expected.sort();

    let list = html.split("const bundledExamples = [").nth(1)
        .and_then(|tail| tail.split("];").next())
        .expect("playground bundledExamples array");
    let mut actual = list.lines()
        .filter_map(|line| line.trim().trim_end_matches(',').strip_prefix('"').and_then(|line| line.strip_suffix('"')))
        .map(str::to_string)
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(expected, actual, "playground bundledExamples must list every top-level N3 example");
}

#[test]
fn every_top_level_n3_example_parses() {
    use std::fs;
    use std::path::Path;

    let examples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut files = fs::read_dir(&examples_dir)
        .expect("read examples directory")
        .map(|entry| entry.expect("read examples entry").path())
        .filter(|path| path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("n3"))
        .collect::<Vec<_>>();
    files.sort();
    assert!(!files.is_empty(), "no top-level N3 examples found");

    for path in files {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {}", path.display(), err));
        let label = path.to_string_lossy();
        parse_n3_with_source(&source, None, Some(label.as_ref()))
            .unwrap_or_else(|err| panic!("example {} is not valid N3: {}", path.display(), err));
    }
}

#[test]
fn log_skolem_is_stable_by_default() {
    let source = r#"
        @prefix : <http://example.org/#> .
        @prefix log: <http://www.w3.org/2000/10/swap/log#> .

        { ("abc" 77) log:skolem ?id . } => { :Result :skolem ?id . } .
    "#;
    let out1 = reason(source).unwrap();
    let out2 = reason(source).unwrap();
    assert_eq!(out1, out2, "log:skolem should be stable by default");
    assert!(out1.contains(":Result :skolem genid:"), "{}", out1);
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
fn dog_license_collect_all_is_scoped_by_subject() {
    let out = reason(include_str!("../examples/dog.n3")).unwrap();
    assert!(out.contains(":alice :mustHave :dogLicense"), "{}", out);
    assert!(
        !out.contains(":bob :mustHave :dogLicense"),
        "log:collectAllIn must count dogs per bound subject, not globally:\n{}",
        out
    );
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

    for (name, source_path, golden_path) in cases {
        progress_line(&format!("example examples/{}.n3 ... running", name));
        let started = std::time::Instant::now();

        let source = fs::read_to_string(&source_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {}", source_path.display(), err));
        let input_path = root.join("examples").join("input").join(format!("{}.trig", name));
        let input = if input_path.exists() {
            Some(fs::read_to_string(&input_path)
                .unwrap_or_else(|err| panic!("failed to read {}: {}", input_path.display(), err)))
        } else {
            None
        };
        let golden = fs::read_to_string(&golden_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {}", golden_path.display(), err));

        let (tx, rx) = std::sync::mpsc::channel();
        let thread_name = name.clone();
        std::thread::spawn(move || {
            let mut sources = vec![("rules", source.as_str())];
            if let Some(input) = input.as_ref() {
                sources.push(("input", input.as_str()));
            }
            let result = check_golden_documents(&thread_name, sources, &golden);
            let _ = tx.send(result);
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
            Ok(Ok(())) => progress_line(&format!(
                "example examples/{}.n3 ... {} ({:.3}s)",
                name,
                green("pass"),
                started.elapsed().as_secs_f64()
            )),
            Ok(Err(msg)) => {
                progress_line(&format!(
                    "example examples/{}.n3 ... {} ({:.3}s)",
                    name,
                    red("fail"),
                    started.elapsed().as_secs_f64()
                ));
                panic!("{}", msg);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                progress_line(&format!(
                    "example examples/{}.n3 ... {} ({:.3}s)",
                    name,
                    red("fail"),
                    started.elapsed().as_secs_f64()
                ));
                panic!(
                    "{} exceeded the {:.0}s per-example golden-test limit after {:.3}s",
                    name,
                    timeout.as_secs_f64(),
                    started.elapsed().as_secs_f64()
                );
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                progress_line(&format!(
                    "example examples/{}.n3 ... {} ({:.3}s)",
                    name,
                    red("fail"),
                    started.elapsed().as_secs_f64()
                ));
                panic!("{} golden-test worker terminated without reporting a result", name);
            }
        }
    }
}


#[test]
fn rdf12_turtle_profile_parses_lists_through_shared_parser() {
    let doc = eyeron::parse_rdf12(
        r#"PREFIX : <http://example.org/>
:s :p (1 2) ."#,
        Some("http://example.org/base"),
        eyeron::RdfFormat::Turtle,
    ).unwrap();
    let json = eyeron::rdf12_json(&doc);
    assert!(json.contains("http://www.w3.org/1999/02/22-rdf-syntax-ns#first"), "{}", json);
    assert!(json.contains("http://www.w3.org/1999/02/22-rdf-syntax-ns#rest"), "{}", json);
}

#[test]
fn rdf12_trig_profile_materializes_named_graphs_as_quads() {
    let doc = eyeron::parse_rdf12(
        r#"PREFIX : <http://example.org/>
:g { :s :p :o . }"#,
        None,
        eyeron::RdfFormat::Trig,
    ).unwrap();
    let json = eyeron::rdf12_json(&doc);
    assert!(json.contains("\"graph\":{\"termType\":\"NamedNode\",\"value\":\"http://example.org/g\"}"), "{}", json);
}

#[test]
fn rdf12_parenthesized_triple_terms_use_formula_term_representation() {
    let doc = eyeron::parse_rdf12(
        r#"PREFIX : <http://example.org/>
:s :p <<(:a :b :c)>> ."#,
        None,
        eyeron::RdfFormat::Turtle,
    ).unwrap();
    let json = eyeron::rdf12_json(&doc);
    assert!(json.contains("\"termType\":\"Quad\""), "{}", json);
    assert!(json.contains("http://example.org/a"), "{}", json);
}

#[test]
fn reasoner_reports_iteration_limit_instead_of_silent_partial_success() {
    use eyeron::{CompletionStatus, ReasonerLimit};

    let doc = parse_n3(
        r#"
            @prefix : <http://example.org/> .
            :a :p :b .
            { :a :p :b } => { :a :q :c } .
        "#,
        None,
    )
    .unwrap();
    let result = reason_document(
        &doc,
        &ReasonerOptions { max_iterations: 0, ..ReasonerOptions::default() },
    );

    assert_eq!(result.status, CompletionStatus::Incomplete);
    assert_eq!(result.statistics.iterations, 0);
    assert!(result.limits_reached.contains(&ReasonerLimit::Iterations));
    assert!(result.derived.is_empty());
}

#[test]
fn reasoner_reports_match_step_limit() {
    use eyeron::{CompletionStatus, ReasonerLimit};

    let doc = parse_n3(
        r#"
            @prefix : <http://example.org/> .
            :a :p :b .
            :a :q :c .
            { :a :p :b . :a :q :c } => { :a :r :d } .
        "#,
        None,
    )
    .unwrap();
    let result = reason_document(
        &doc,
        &ReasonerOptions { max_match_steps: 0, ..ReasonerOptions::default() },
    );

    assert_eq!(result.status, CompletionStatus::Incomplete);
    assert!(result.limits_reached.contains(&ReasonerLimit::MatchSteps));
    assert!(result.derived.is_empty());
}

#[test]
fn resource_builtin_uses_deterministic_hello_fixture() {
    let doc = parse_n3(
        r#"
            @prefix : <http://example.org/> .
            @prefix log: <http://www.w3.org/2000/10/swap/log#> .
            { <http://example.org/HELLO.txt> log:content ?text } => { :result :text ?text } .
        "#,
        None,
    )
    .unwrap();
    let result = reason_document(&doc, &ReasonerOptions::default());

    assert!(result.is_complete(), "{:?}", result.errors);
    assert!(result_to_string(&doc.prefixes, &result.derived).contains(":result :text \"Hello, world!\\n\""));
}

#[test]
fn unbound_not_includes_constructs_an_existential_witness_formula() {
    use eyeron::Term;

    let doc = parse_n3(
        r#"
            @prefix : <http://example.org/> .
            @prefix log: <http://www.w3.org/2000/10/swap/log#> .
            { ?scope log:notIncludes { :a :b :c } } => { :result :scope ?scope } .
        "#,
        None,
    )
    .unwrap();
    let result = reason_document(&doc, &ReasonerOptions::default());

    assert!(result.is_complete(), "{:?}", result.errors);
    assert!(result.derived.iter().any(|triple| {
        triple.s == Term::iri("http://example.org/result")
            && triple.p == Term::iri("http://example.org/scope")
            && matches!(&triple.o, Term::Formula(items) if !items.is_empty())
    }));
}

#[test]
fn regex_builtins_use_general_regex_matching() {
    let source = r#"
        @prefix : <http://example.org/> .
        @prefix string: <http://www.w3.org/2000/10/swap/string#> .
        { "abc123" string:matches "^[a-z]+[0-9]+$" } => { :result :value "matched" } .
    "#;
    let doc = parse_n3(source, None).unwrap();
    let result = reason_document(&doc, &ReasonerOptions::default());

    assert!(result.is_complete(), "{:?}", result.errors);
    assert!(result_to_string(&doc.prefixes, &result.derived).contains(":result :value \"matched\""));
}

#[test]
fn lookaround_regex_syntax_uses_compatibility_matching() {
    let doc = parse_n3(
        r#"
            @prefix : <http://example.org/> .
            @prefix string: <http://www.w3.org/2000/10/swap/string#> .
            { "abc" string:matches "^(?=a)abc$" } => { :result :value "matched" } .
        "#,
        None,
    )
    .unwrap();
    let result = reason_document(&doc, &ReasonerOptions::default());

    assert!(result.is_complete(), "{:?}", result.errors);
    assert!(result_to_string(&doc.prefixes, &result.derived).contains(":result :value \"matched\""));
}

#[test]
fn proof_output_marks_missing_support_as_unproven() {
    use eyeron::{CompletionStatus, ReasonerResult, ReasonerStatistics, Rule, Term, Triple};
    use eyeron::reasoner::DerivedFact;
    use std::collections::BTreeMap;

    let missing = Triple::new(Term::iri("http://example.org/a"), Term::iri("http://example.org/p"), Term::iri("http://example.org/b"));
    let derived = Triple::new(Term::iri("http://example.org/a"), Term::iri("http://example.org/q"), Term::iri("http://example.org/c"));
    let rule = Rule::new(vec![missing.clone()], vec![derived.clone()], true);
    let proof = DerivedFact {
        fact: derived.clone(),
        rule: rule.clone(),
        premises: vec![missing],
        bindings: BTreeMap::new(),
    };
    let result = ReasonerResult {
        status: CompletionStatus::Complete,
        limits_reached: Vec::new(),
        errors: Vec::new(),
        statistics: ReasonerStatistics::default(),
        explicit: Vec::new(),
        explicit_sources: BTreeMap::new(),
        derived: vec![derived.clone()],
        closure: vec![derived],
        proofs: vec![proof],
        rules: vec![rule],
    };

    let output = proof_to_n3(&BTreeMap::new(), &result);
    assert!(output.contains("pe:unproven"), "{}", output);
    assert!(!output.contains("pe:fact \"<unknown>\""), "{}", output);
}

#[test]
fn regex_replacement_preserves_n3_dollar_and_backslash_escapes() {
    let doc = parse_n3(
        r#"
            @prefix : <http://example.org/> .
            @prefix string: <http://www.w3.org/2000/10/swap/string#> .
            { ("abcd" "b" "\\$\\\\") string:replace "a$\\cd" } => { :result :value "matched" } .
        "#,
        None,
    )
    .unwrap();
    let result = reason_document(&doc, &ReasonerOptions::default());

    assert!(result.is_complete(), "{:?}", result.errors);
    assert!(result_to_string(&doc.prefixes, &result.derived).contains(":result :value \"matched\""));
}

#[test]
fn high_level_reason_does_not_fabricate_unknown_resource_content() {
    let output = eyeron::reason(
        r#"
            @prefix : <http://example.org/> .
            @prefix log: <http://www.w3.org/2000/10/swap/log#> .
            { <http://example.org/data.txt> log:content ?text } => { :result :text ?text } .
        "#,
    )
    .unwrap();

    assert!(output.is_empty(), "{}", output);
}

#[test]
fn proof_output_recognizes_compatible_lookaround_builtin() {
    use eyeron::{CompletionStatus, ReasonerResult, ReasonerStatistics, Rule, Term, Triple};
    use eyeron::reasoner::DerivedFact;
    use std::collections::BTreeMap;

    let compatible_builtin = Triple::new(
        Term::literal("abc"),
        Term::iri("http://www.w3.org/2000/10/swap/string#matches"),
        Term::literal("^(?=a)abc$"),
    );
    let derived = Triple::new(
        Term::iri("http://example.org/result"),
        Term::iri("http://example.org/value"),
        Term::literal("matched"),
    );
    let rule = Rule::new(vec![compatible_builtin.clone()], vec![derived.clone()], true);
    let proof = DerivedFact {
        fact: derived.clone(),
        rule: rule.clone(),
        premises: vec![compatible_builtin],
        bindings: BTreeMap::new(),
    };
    let result = ReasonerResult {
        status: CompletionStatus::Complete,
        limits_reached: Vec::new(),
        errors: Vec::new(),
        statistics: ReasonerStatistics::default(),
        explicit: Vec::new(),
        explicit_sources: BTreeMap::new(),
        derived: vec![derived.clone()],
        closure: vec![derived],
        proofs: vec![proof],
        rules: vec![rule],
    };

    let output = proof_to_n3(&BTreeMap::new(), &result);
    assert!(output.contains("pe:builtin <http://www.w3.org/2000/10/swap/string#matches>"), "{}", output);
    assert!(!output.contains("pe:unproven"), "{}", output);
}

#[test]
fn log_name_of_remains_an_ordinary_graph_predicate() {
    use eyeron::Term;

    let doc = parse_n3(
        r#"
            @prefix : <http://example.org/> .
            @prefix log: <http://www.w3.org/2000/10/swap/log#> .
            :payload log:nameOf { :subject :predicate :object } .
            { :payload log:nameOf ?formula } => { :result :formula ?formula } .
        "#,
        None,
    )
    .unwrap();
    let result = reason_document(&doc, &ReasonerOptions::default());

    assert!(result.is_complete(), "{:?}", result.errors);
    assert!(result.derived.iter().any(|triple| {
        triple.s == Term::iri("http://example.org/result")
            && triple.p == Term::iri("http://example.org/formula")
            && matches!(&triple.o, Term::Formula(_))
    }));
}

#[test]
fn unknown_predicate_in_builtin_namespace_remains_an_ordinary_predicate() {
    let doc = parse_n3(
        r#"
            @prefix : <http://example.org/> .
            @prefix log: <http://www.w3.org/2000/10/swap/log#> .
            { :subject log:unknownBuiltin ?value } => { :result :value ?value } .
        "#,
        None,
    )
    .unwrap();
    let result = reason_document(&doc, &ReasonerOptions::default());

    assert!(result.is_complete(), "{:?}", result.errors);
    assert!(result.derived.is_empty());
}
