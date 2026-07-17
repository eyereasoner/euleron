use eyeron::{
    parse_n3, parse_n3_with_source, parse_rdf12, proof_to_n3, rdf_result_to_string, reason,
    reason_document, result_to_string, Document, RdfFormat, ReasonerOptions,
};

fn check_golden_non_prefix_lines(
    name: &str,
    source: &str,
    golden: &str,
) -> std::result::Result<(), String> {
    let out = reason(source).map_err(|err| format!("{} failed: {}", name, err))?;
    for expected in stable_golden_lines(golden) {
        if !out.contains(expected) {
            return Err(format!(
                "{} missing golden line `{}`\nactual:\n{}",
                name, expected, out
            ));
        }
    }
    Ok(())
}

fn assert_golden_non_prefix_lines(name: &str, source: &str, golden: &str) {
    check_golden_non_prefix_lines(name, source, golden).unwrap_or_else(|msg| panic!("{}", msg));
}

#[test]
fn n3_lists_remain_first_class_in_rule_conclusions() {
    let source = r#"
        @prefix : <http://example.org/>.
        { :input :value ?Value. } => { (:result ?Value) :contains (:answer ?Value). }.
        :input :value 42.
    "#;

    let doc = parse_n3(source, None).unwrap();
    let result = reason_document(&doc, &ReasonerOptions::default());
    let output = result_to_string(&doc.prefixes, &result.derived);

    assert!(output.contains("(:result 42) :contains (:answer 42)"), "{output}");
    assert!(!output.contains("rdf:first"), "{output}");
    assert!(!output.contains("rdf:rest"), "{output}");
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
    assert!(
        doc.facts
            .iter()
            .any(|t| matches!(&t.p, eyeron::Term::Iri(p) if p == reifies)),
        "{:#?}",
        doc.facts
    );
    assert!(
        doc.facts
            .iter()
            .any(|t| matches!(&t.p, eyeron::Term::Iri(p) if p == "http://example.org/source")),
        "{:#?}",
        doc.facts
    );
}

#[test]
fn rdf12_parenthesized_triple_terms_remain_terms() {
    let input = r#"
        PREFIX : <http://example.org/>
        :s :p <<(:a :b :c)>> .
    "#;
    let doc = parse_rdf12(input, Some("http://example.org/base"), RdfFormat::Turtle).unwrap();
    assert!(
        doc.facts
            .iter()
            .any(|t| matches!(&t.o, eyeron::Term::Formula(inner) if inner.len() == 1)),
        "{:#?}",
        doc.facts
    );
}

#[test]
fn age_example_supports_current_time_date_difference_and_duration_comparison() {
    use std::fs;
    use std::path::Path;

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/age.n3");
    let source = fs::read_to_string(&path).expect("read examples/age.n3");
    let doc = parse_n3_with_source(&source, None, Some("age.n3")).expect("parse age example");
    let result = reason_document(
        &doc,
        &ReasonerOptions {
            proof: true,
            ..ReasonerOptions::default()
        },
    );

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
        (
            "backward",
            include_str!("../examples/backward.n3"),
            include_str!("../examples/output/backward.n3"),
        ),
        (
            "schema-foaf-mapping",
            include_str!("../examples/schema-foaf-mapping.n3"),
            include_str!("../examples/output/schema-foaf-mapping.n3"),
        ),
        (
            "similar",
            include_str!("../examples/similar.n3"),
            include_str!("../examples/output/similar.n3"),
        ),
        (
            "monkey",
            include_str!("../examples/monkey.n3"),
            include_str!("../examples/output/monkey.n3"),
        ),
        (
            "rdf-list",
            include_str!("../examples/rdf-list.n3"),
            include_str!("../examples/output/rdf-list.n3"),
        ),
        (
            "rule-matching",
            include_str!("../examples/rule-matching.n3"),
            include_str!("../examples/output/rule-matching.n3"),
        ),
        (
            "log-not-includes",
            include_str!("../examples/log-not-includes.n3"),
            include_str!("../examples/output/log-not-includes.n3"),
        ),
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
    assert!(
        !out.contains(":bob :childOf :alice"),
        "derived backward rules must not materialize the goal fact:\n{}",
        out
    );
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
        (
            "dog",
            include_str!("../examples/dog.n3"),
            include_str!("../examples/output/dog.n3"),
        ),
        (
            "log-collect-all-in",
            include_str!("../examples/log-collect-all-in.n3"),
            include_str!("../examples/output/log-collect-all-in.n3"),
        ),
        (
            "list-iterate",
            include_str!("../examples/list-iterate.n3"),
            include_str!("../examples/output/list-iterate.n3"),
        ),
        (
            "list-map",
            include_str!("../examples/list-map.n3"),
            include_str!("../examples/output/list-map.n3"),
        ),
    ];
    for (name, source, golden) in cases {
        assert_golden_non_prefix_lines(name, source, golden);
    }
}

#[test]
fn rdf12_turtle_profile_parses_lists_through_shared_parser() {
    let doc = eyeron::parse_rdf12(
        r#"PREFIX : <http://example.org/>
:s :p (1 2) ."#,
        Some("http://example.org/base"),
        eyeron::RdfFormat::Turtle,
    )
    .unwrap();
    let json = eyeron::rdf12_json(&doc);
    assert!(
        json.contains("http://www.w3.org/1999/02/22-rdf-syntax-ns#first"),
        "{}",
        json
    );
    assert!(
        json.contains("http://www.w3.org/1999/02/22-rdf-syntax-ns#rest"),
        "{}",
        json
    );
}

#[test]
fn rdf12_trig_profile_materializes_named_graphs_as_quads() {
    let doc = eyeron::parse_rdf12(
        r#"PREFIX : <http://example.org/>
:g { :s :p :o . }"#,
        None,
        eyeron::RdfFormat::Trig,
    )
    .unwrap();
    let json = eyeron::rdf12_json(&doc);
    assert!(
        json.contains("\"graph\":{\"termType\":\"NamedNode\",\"value\":\"http://example.org/g\"}"),
        "{}",
        json
    );
}

#[test]
fn rdf12_parenthesized_triple_terms_use_formula_term_representation() {
    let doc = eyeron::parse_rdf12(
        r#"PREFIX : <http://example.org/>
:s :p <<(:a :b :c)>> ."#,
        None,
        eyeron::RdfFormat::Turtle,
    )
    .unwrap();
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
        &ReasonerOptions {
            max_iterations: 0,
            ..ReasonerOptions::default()
        },
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
        &ReasonerOptions {
            max_match_steps: 0,
            ..ReasonerOptions::default()
        },
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
    assert!(result_to_string(&doc.prefixes, &result.derived)
        .contains(":result :text \"Hello, world!\\n\""));
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
    use eyeron::reasoner::DerivedFact;
    use eyeron::{CompletionStatus, ReasonerResult, ReasonerStatistics, Rule, Term, Triple};
    use std::collections::BTreeMap;

    let missing = Triple::new(
        Term::iri("http://example.org/a"),
        Term::iri("http://example.org/p"),
        Term::iri("http://example.org/b"),
    );
    let derived = Triple::new(
        Term::iri("http://example.org/a"),
        Term::iri("http://example.org/q"),
        Term::iri("http://example.org/c"),
    );
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
    use eyeron::reasoner::DerivedFact;
    use eyeron::{CompletionStatus, ReasonerResult, ReasonerStatistics, Rule, Term, Triple};
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
    let rule = Rule::new(
        vec![compatible_builtin.clone()],
        vec![derived.clone()],
        true,
    );
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
    assert!(
        output.contains("pe:builtin <http://www.w3.org/2000/10/swap/string#matches>"),
        "{}",
        output
    );
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

#[test]
fn eyeling_datatype_inspection_builtins_drive_generated_rules() {
    let source = r#"
        @prefix : <http://example.org/> .
        @prefix cdt: <https://w3id.org/cdt/> .
        @prefix dt: <https://eyereasoner.github.io/eyeling/datatype#> .

        :measurement :speed "36 local-km/h"^^cdt:speed .
        {
            ?measurement :speed ?literal .
            ?literal dt:datatype ?datatype .
            ?literal dt:lexicalForm ?lexical .
        } => {
            ?literal :inspectedAs (?datatype ?lexical) .
        } .
    "#;

    let output = reason(source).unwrap();
    assert!(
        output.contains("\"36 local-km/h\"^^cdt:speed :inspectedAs (cdt:speed \"36 local-km/h\")"),
        "{output}"
    );
}

#[test]
fn blank_scope_log_not_includes_guards_generated_rules() {
    let source = r#"
        @prefix : <http://example.org/> .
        @prefix log: <http://www.w3.org/2000/10/swap/log#> .

        :input :value :ordinary .
        :blocked :guard true .
        {
            ?subject :value ?value .
            _:scope log:notIncludes { ?subject :guard true } .
        } => {
            ?subject :accepted ?value .
        } .
        :blocked :value :must-not-pass .
    "#;

    let output = reason(source).unwrap();
    assert!(output.contains(":input :accepted :ordinary"), "{output}");
    assert!(!output.contains(":blocked :accepted"), "{output}");
}

#[test]
fn blank_scope_log_not_includes_uses_existing_outer_bindings() {
    let source = r#"
        @prefix : <http://example.org/> .
        @prefix log: <http://www.w3.org/2000/10/swap/log#> .

        :output :isGenerated true .
        :input :value :first .
        {
            ?subject ?property ?value .
            _:scope log:notIncludes { ?property :isGenerated true } .
        } => {
            ?subject :acceptedProperty ?property .
        } .
    "#;

    let output = reason(source).unwrap();
    assert!(!output.contains(":acceptedProperty :output"), "{output}");
}
