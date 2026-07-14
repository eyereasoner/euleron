#![allow(dead_code, unused_imports)]
use euleron::ast::{Literal, Term, Triple, LOG_NAME_OF, RDF_FIRST, RDF_NIL, RDF_REST, RDF_TYPE};
use euleron::{parse_rdf12, Document, RdfFormat};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use serde_json::Value as JsonValue;

const MF: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#";
const RDFT: &str = "http://www.w3.org/ns/rdftest#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

const MF_ENTRIES: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#entries";
const MF_INCLUDE: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#include";
const MF_NAME: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#name";
const MF_ACTION: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#action";
const MF_RESULT: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#result";
const MF_ENTAILMENT_REGIME: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#entailmentRegime";
const MF_RECOGNIZED_DATATYPES: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#recognizedDatatypes";
const MF_UNRECOGNIZED_DATATYPES: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#unrecognizedDatatypes";

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_INT: &str = "http://www.w3.org/2001/XMLSchema#int";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_FLOAT: &str = "http://www.w3.org/2001/XMLSchema#float";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
const RDF_PROPERTY: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property";
const RDF_XML_LITERAL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#XMLLiteral";
const RDF_HTML: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#HTML";
const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
const RDF_DIR_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString";
const RDF_JSON: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#JSON";
const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const RDF_TRIPLE_TERM: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#TripleTerm";
const RDF_PROPOSITION: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Proposition";
const RDFS_PROPOSITION: &str = "http://www.w3.org/2000/01/rdf-schema#Proposition";
const RDFS_RESOURCE: &str = "http://www.w3.org/2000/01/rdf-schema#Resource";
const RDFS_CLASS: &str = "http://www.w3.org/2000/01/rdf-schema#Class";
const RDFS_LITERAL: &str = "http://www.w3.org/2000/01/rdf-schema#Literal";
const RDFS_DATATYPE: &str = "http://www.w3.org/2000/01/rdf-schema#Datatype";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const RDFS_CONTAINER_MEMBERSHIP_PROPERTY: &str = "http://www.w3.org/2000/01/rdf-schema#ContainerMembershipProperty";
const RDFS_MEMBER: &str = "http://www.w3.org/2000/01/rdf-schema#member";

const DEFAULT_W3C_RDF_CACHE_DIR: &str = "tests/w3c_rdf/rdf-tests";

const DEFAULT_MANIFESTS: &[&str] = &[
    "https://w3c.github.io/rdf-tests/rdf/rdf11/rdf-n-triples/manifest.ttl",
    "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-n-triples/syntax/manifest.ttl",
    "https://w3c.github.io/rdf-tests/rdf/rdf11/rdf-n-quads/manifest.ttl",
    "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-n-quads/syntax/manifest.ttl",
    "https://w3c.github.io/rdf-tests/rdf/rdf11/rdf-mt/manifest.ttl",
    "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-semantics/manifest.ttl",
    "https://w3c.github.io/rdf-tests/rdf/rdf11/rdf-turtle/manifest.ttl",
    "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-turtle/eval/manifest.ttl",
    "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-turtle/syntax/manifest.ttl",
    "https://w3c.github.io/rdf-tests/rdf/rdf11/rdf-trig/manifest.ttl",
    "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-trig/eval/manifest.ttl",
    "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-trig/syntax/manifest.ttl",
];

#[derive(Debug, Default)]
struct Options {
    resources: Vec<String>,
    output: PathBuf,
    no_report: bool,
    earl_stdout: bool,
    json: bool,
    quiet: bool,
    verbose: bool,
    filter: Option<String>,
    list: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct ManifestCase {
    id: Term,
    name: String,
    types: Vec<String>,
    manifest: String,
    action: Option<String>,
    result: Option<String>,
    result_kind: ResultKind,
    entailment_regime: Option<String>,
    recognized_datatypes: Vec<String>,
    unrecognized_datatypes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultKind { Resource, False, None }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaseKind { PositiveSyntax, NegativeSyntax, Eval, PositiveEntailment, NegativeEntailment, Unknown }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status { Pass, Fail, Skip }

#[derive(Debug, Clone)]
struct CaseResult {
    case: ManifestCase,
    status: Status,
    message: String,
    duration_ms: u128,
}

#[derive(Debug, Clone)]
struct ManifestRun {
    source: String,
    manifests_loaded: Vec<ManifestLoadInfo>,
    results: Vec<CaseResult>,
    duration_ms: u128,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct ManifestLoadInfo {
    resource: String,
    include_count: usize,
    test_count: usize,
}

#[derive(Debug, Default, Clone)]
struct Counts { total: usize, pass: usize, fail: usize, skip: usize }

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum CTerm {
    DefaultGraph,
    Iri(String),
    Blank(String),
    Literal { value: String, datatype: String, language: String },
    Triple(Box<CTripleTerm>),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct CTripleTerm { s: CTerm, p: CTerm, o: CTerm }

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct CQuad { s: CTerm, p: CTerm, o: CTerm, g: CTerm }

struct Runner {
    cache: HashMap<String, String>,
    parsed_docs: HashMap<String, Result<Document, String>>,
    disk_cache_dir: PathBuf,
    offline: bool,
    refresh_cache: bool,
}

pub fn run_default_suite() -> Result<(), String> {
    let counts = run_options(options_from_env(Vec::new(), true))?;
    let expected = if has_filter() { None } else { Some(1170) };
    assert_clean_counts("w3c_rdf_13_all_manifests_1170_earl_report", &counts, expected)
}

pub fn refresh_requested() -> bool { env_flag("EULERON_W3C_RDF_REFRESH") }

pub fn run_manifest_suite_for_test(label: &str, resource: &str, expected_total: usize) -> Result<(), String> {
    let counts = run_options(options_from_env(vec![resource.to_string()], false))?;
    let expected = if has_filter() { None } else { Some(expected_total) };
    assert_clean_counts(label, &counts, expected)
}

fn options_from_env(resources: Vec<String>, write_report: bool) -> Options {
    let output = std::env::var("EULERON_W3C_RDF_EARL")
        .unwrap_or_else(|_| "reports/w3c-rdf-earl.ttl".to_string());
    let filter = std::env::var("EULERON_W3C_RDF_FILTER")
        .ok()
        .filter(|value| !value.trim().is_empty());
    Options {
        resources,
        output: PathBuf::from(output),
        no_report: !write_report,
        filter,
        quiet: env_flag("EULERON_W3C_RDF_QUIET"),
        verbose: env_flag("EULERON_W3C_RDF_VERBOSE"),
        ..Options::default()
    }
}

fn has_filter() -> bool {
    std::env::var("EULERON_W3C_RDF_FILTER").map(|value| !value.trim().is_empty()).unwrap_or(false)
}

fn assert_clean_counts(label: &str, counts: &Counts, expected_total: Option<usize>) -> Result<(), String> {
    if counts.fail > 0 || counts.skip > 0 || counts.pass != counts.total {
        return Err(format!(
            "{label} did not reach 100%: ok={} count={} failed={} skipped={}",
            counts.pass, counts.total, counts.fail, counts.skip
        ));
    }
    if let Some(expected_total) = expected_total {
        if counts.total != expected_total {
            return Err(format!(
                "{label} ran an unexpected number of W3C RDF tests: ok={} count={} expected={}",
                counts.pass, counts.total, expected_total
            ));
        }
    }
    Ok(())
}

fn run_options(opt: Options) -> Result<Counts, String> {
    let resources = if opt.resources.is_empty() {
        DEFAULT_MANIFESTS.iter().map(|s| s.to_string()).collect::<Vec<_>>()
    } else {
        opt.resources.iter().map(|s| normalize_resource(s)).collect::<Vec<_>>()
    };
    let mut runner = Runner::from_env();
    let started = Instant::now();
    let mut runs = Vec::new();
    for (index, resource) in resources.iter().enumerate() {
        if !opt.quiet { eprintln!("== W3C RDF manifest {}/{}: {}", index + 1, resources.len(), resource); }
        let run = runner.run_manifest(resource, &opt)?;
        if !opt.quiet { eprintln!("{}", format_manifest_summary(&run)); }
        runs.push(run);
    }
    let counts = total_counts(&runs);
    let duration_ms = started.elapsed().as_millis();
    let earl = rdf_manifests_to_earl(&runs);
    if !opt.no_report {
        if let Some(parent) = opt.output.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
        fs::write(&opt.output, format!("{earl}\n")).map_err(|e| format!("failed to write {}: {e}", opt.output.display()))?;
        if !opt.quiet { eprintln!("EARL report: {}", opt.output.display()); }
    }
    if opt.earl_stdout { print!("{earl}\n"); }
    else if opt.json { print_json_summary(&runs, &counts, duration_ms); }
    else if !opt.quiet {
        println!("== Total");
        println!("{} {}/{} tests passed across {} manifest(s) ({} ms)", if counts.fail == 0 { "OK" } else { "FAIL" }, counts.pass, counts.total, runs.len(), duration_ms);
        if counts.skip > 0 { println!("{} skipped", counts.skip); }
    }
    Ok(counts)
}

fn parse_args(args: Vec<String>) -> Result<Options, String> {
    let mut opt = Options { output: PathBuf::from("reports/w3c-rdf-earl.ttl"), ..Options::default() };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => { print_help(); std::process::exit(0); }
            "--list" => opt.list = true,
            "--quiet" => opt.quiet = true,
            "--verbose" => opt.verbose = true,
            "--json" => opt.json = true,
            "--earl" => opt.earl_stdout = true,
            "--no-report" => opt.no_report = true,
            "--output" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| "--output requires a path".to_string())?;
                opt.output = PathBuf::from(value);
            }
            "--filter" => {
                i += 1;
                let value = args.get(i).ok_or_else(|| "--filter requires text".to_string())?;
                opt.filter = Some(value.clone());
            }
            other if other.starts_with('-') => return Err(format!("unknown option: {other}")),
            resource => opt.resources.push(resource.to_string()),
        }
        i += 1;
    }
    Ok(opt)
}

fn print_help() {
    println!("Rust-only W3C RDF 1.x manifest test runner for Euleron");
    println!();
    println!("Usage:");
    println!("  cargo test --test w3c_rdf");
    println!();
    println!("By default, runs the same 12 W3C RDF manifests from the local tests/w3c_rdf/rdf-tests mirror and writes reports/w3c-rdf-earl.ttl.");
    println!("Network is disabled by default for speed; use EULERON_W3C_RDF_REFRESH=1 to bootstrap or refresh the local mirror.");
    println!();
    println!("Options:");
    println!("  --output PATH   EARL report path (default: reports/w3c-rdf-earl.ttl)");
    println!("  --earl          Print the EARL report to stdout as well");
    println!("  --json          Print a small JSON summary to stdout");
    println!("  --no-report     Do not write the EARL report file");
    println!("  --filter TEXT   Run only tests whose name, type, id, action, or manifest contains TEXT");
    println!("  --quiet         Suppress progress lines");
    println!("  --verbose       Print every test case, not just the notation3tests-style progress cadence");
    println!("  --list          List default manifest URLs");
}

impl Runner {
    fn from_env() -> Self {
        let disk_cache_dir = env::var("EULERON_W3C_RDF_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_W3C_RDF_CACHE_DIR));
        let refresh_cache = env_flag("EULERON_W3C_RDF_REFRESH");
        let online = refresh_cache || env_flag("EULERON_W3C_RDF_ONLINE");
        let offline = env_flag("EULERON_W3C_RDF_OFFLINE") || !online;
        Runner { cache: HashMap::new(), parsed_docs: HashMap::new(), disk_cache_dir, offline, refresh_cache }
    }

    fn run_manifest(&mut self, resource: &str, opt: &Options) -> Result<ManifestRun, String> {
        let start = Instant::now();
        let mut seen = BTreeSet::new();
        let mut infos = Vec::new();
        let cases = self.load_manifest_recursive(resource, &mut seen, &mut infos)?;
        let mut results = Vec::new();
        for (idx, case) in cases.into_iter().enumerate() {
            if let Some(filter) = &opt.filter {
                let hay = format!("{} {} {} {} {}", case.name, term_label(&case.id), case.types.join(" "), case.action.clone().unwrap_or_default(), case.manifest);
                if !hay.contains(filter) { continue; }
            }
            let item = self.run_case(case);
            if !opt.quiet && should_print_case_progress(&item, idx, opt.verbose) {
                eprintln!("{}", format_progress_line(&item, idx));
            }
            results.push(item);
        }
        Ok(ManifestRun { source: resource.to_string(), manifests_loaded: infos, results, duration_ms: start.elapsed().as_millis() })
    }

    fn load_manifest_recursive(&mut self, resource: &str, seen: &mut BTreeSet<String>, infos: &mut Vec<ManifestLoadInfo>) -> Result<Vec<ManifestCase>, String> {
        let resource = normalize_resource(resource);
        if !seen.insert(resource.clone()) { return Ok(Vec::new()); }
        let text = self.read_resource(&resource)?;
        let doc = parse_rdf12(&text, Some(&resource), RdfFormat::Turtle)
            .map_err(|err| format!("failed to parse manifest {resource}: {err}"))?;
        let includes = manifest_includes(&doc, &resource);
        let mut cases = manifest_cases(&doc, &resource);
        infos.push(ManifestLoadInfo { resource: resource.clone(), include_count: includes.len(), test_count: cases.len() });
        for include in includes {
            cases.extend(self.load_manifest_recursive(&include, seen, infos)?);
        }
        Ok(cases)
    }

    fn read_resource(&mut self, resource: &str) -> Result<String, String> {
        if let Some(cached) = self.cache.get(resource) { return Ok(cached.clone()); }
        let text = if is_url(resource) {
            self.read_url_resource(resource)?
        } else {
            fs::read_to_string(resource).map_err(|err| format!("failed to read {resource}: {err}"))?
        };
        self.cache.insert(resource.to_string(), text.clone());
        Ok(text)
    }

    fn read_url_resource(&mut self, resource: &str) -> Result<String, String> {
        if !self.refresh_cache {
            if let Some(path) = self.cache_path_for_url(resource) {
                if path.exists() {
                    return fs::read_to_string(&path)
                        .map_err(|err| format!("failed to read cached {} for {resource}: {err}", path.display()));
                }
            }
        }
        if self.offline {
            let expected = self.cache_path_for_url(resource)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| self.disk_cache_dir.display().to_string());
            return Err(format!(
                "local W3C RDF mirror miss for {resource}; expected {expected}. \
                 The W3C RDF conformance test is intentionally local-only so `cargo test` stays fast. \
                 Run once with EULERON_W3C_RDF_REFRESH=1 to populate the mirror, then commit tests/w3c_rdf/rdf-tests/."
            ));
        }
        let text = match fetch_url(resource) {
            Ok(text) => text,
            Err(primary) => {
                if let Some(fallback) = github_raw_fallback(resource) {
                    fetch_url(&fallback).map_err(|secondary| format!("failed to fetch {resource}: {primary}; fallback {fallback}: {secondary}"))?
                } else {
                    return Err(format!("failed to fetch {resource}: {primary}"));
                }
            }
        };
        if let Some(path) = self.cache_path_for_url(resource) {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|err| format!("failed to create W3C RDF cache directory {}: {err}", parent.display()))?;
            }
            fs::write(&path, &text).map_err(|err| format!("failed to write W3C RDF cache file {}: {err}", path.display()))?;
        }
        Ok(text)
    }

    fn cache_path_for_url(&self, resource: &str) -> Option<PathBuf> {
        w3c_rdf_relative_path(resource).map(|relative| self.disk_cache_dir.join(relative))
    }

    fn run_case(&mut self, case: ManifestCase) -> CaseResult {
        let start = Instant::now();
        let outcome = self.run_case_inner(&case);
        let duration_ms = start.elapsed().as_millis();
        match outcome {
            Ok((status, message)) => CaseResult { case, status, message, duration_ms },
            Err(message) => CaseResult { case, status: Status::Fail, message, duration_ms },
        }
    }

    fn run_case_inner(&mut self, case: &ManifestCase) -> Result<(Status, String), String> {
        match case_kind(&case.types) {
            CaseKind::PositiveSyntax => {
                let doc = self.parse_action(case)?;
                drop(doc);
                Ok((Status::Pass, "accepted as expected".to_string()))
            }
            CaseKind::NegativeSyntax => match self.parse_action(case) {
                Ok(_) => Err("negative syntax test was accepted".to_string()),
                Err(err) => Ok((Status::Pass, format!("rejected as expected: {err}"))),
            },
            CaseKind::Eval => self.run_eval_case(case),
            CaseKind::PositiveEntailment | CaseKind::NegativeEntailment => self.run_entailment_case(case),
            CaseKind::Unknown => Ok((Status::Skip, "unsupported RDF test type".to_string())),
        }
    }

    fn parse_action(&mut self, case: &ManifestCase) -> Result<Document, String> {
        let action = case.action.as_ref().ok_or_else(|| "missing mf:action".to_string())?;
        let format = format_for(case, action).ok_or_else(|| format!("no parser selected for {action}"))?;
        self.parse_resource_cached(action, format)
    }

    fn parse_graph_resource(&mut self, resource: &str, type_hint: Option<&str>) -> Result<Document, String> {
        let format = format_from_resource(resource).or_else(|| type_hint.and_then(format_from_type))
            .ok_or_else(|| format!("no parser selected for {resource}"))?;
        self.parse_resource_cached(resource, format)
    }

    fn parse_resource_cached(&mut self, resource: &str, format: RdfFormat) -> Result<Document, String> {
        let key = format!("{}\n{:?}", resource, format);
        if let Some(cached) = self.parsed_docs.get(&key) {
            return cached.clone();
        }
        let text = self.read_resource(resource)?;
        let parsed = parse_rdf12(&text, Some(resource), format).map_err(|err| err.to_string());
        self.parsed_docs.insert(key, parsed.clone());
        parsed
    }

    fn run_eval_case(&mut self, case: &ManifestCase) -> Result<(Status, String), String> {
        let action_doc = self.parse_action(case).map_err(|err| format!("action parse failed: {err}"))?;
        let result = case.result.as_ref().ok_or_else(|| "missing mf:result".to_string())?;
        let type_hint = primary_type(&case.types);
        let expected_doc = self.parse_graph_resource(result, type_hint.as_deref())
            .map_err(|err| format!("result parse failed: {err}"))?;
        let actual = dataset_quads(&action_doc);
        let expected = dataset_quads(&expected_doc);
        if graphs_isomorphic(&actual, &expected) {
            Ok((Status::Pass, "parsed graph matches expected result graph".to_string()))
        } else {
            Err(dataset_diff(&actual, &expected))
        }
    }

    fn run_entailment_case(&mut self, case: &ManifestCase) -> Result<(Status, String), String> {
        let action_doc = self.parse_action(case).map_err(|err| format!("action parse failed: {err}"))?;
        let input = dataset_quads(&action_doc);
        let expected = if case.result_kind == ResultKind::False {
            Vec::new()
        } else {
            let result = case.result.as_ref().ok_or_else(|| "missing mf:result".to_string())?;
            dataset_quads(&self.parse_graph_resource(result, primary_type(&case.types).as_deref())?)
        };
        let positive = matches!(case_kind(&case.types), CaseKind::PositiveEntailment);
        let evaluated = evaluate_entailment(&input, &expected, EntailmentOptions {
            positive,
            result_false: case.result_kind == ResultKind::False,
            regime: case.entailment_regime.clone().unwrap_or_else(|| "simple".to_string()),
            recognized_datatypes: case.recognized_datatypes.clone(),
        });
        if evaluated.passed { Ok((Status::Pass, evaluated.message)) } else { Err(evaluated.message) }
    }
}

fn manifest_cases(doc: &Document, manifest: &str) -> Vec<ManifestCase> {
    let mut subjects = Vec::<Term>::new();
    for entries in objects(doc, None, MF_ENTRIES) {
        for item in list_items(entries) { subjects.push(item.clone()); }
    }
    if subjects.is_empty() {
        for t in &doc.facts {
            if matches!(&t.p, Term::Iri(p) if p == RDF_TYPE) {
                if !subjects.contains(&t.s) { subjects.push(t.s.clone()); }
            }
        }
    }
    let mut out = Vec::new();
    for subject in subjects {
        let types = objects(doc, Some(&subject), RDF_TYPE).into_iter().filter_map(iri_value).collect::<Vec<_>>();
        if !types.iter().any(|t| is_supported_type(t)) { continue; }
        let name = first_literal_object(doc, &subject, MF_NAME).unwrap_or_else(|| term_label(&subject));
        let action = first_iri_object(doc, &subject, MF_ACTION);
        let (result, result_kind) = result_object(doc, &subject);
        let recognized_datatypes = first_list_iri_objects(doc, &subject, MF_RECOGNIZED_DATATYPES);
        let unrecognized_datatypes = first_list_iri_objects(doc, &subject, MF_UNRECOGNIZED_DATATYPES);
        let entailment_regime = first_literal_object(doc, &subject, MF_ENTAILMENT_REGIME);
        out.push(ManifestCase {
            id: subject,
            name,
            types,
            manifest: manifest.to_string(),
            action,
            result,
            result_kind,
            entailment_regime,
            recognized_datatypes,
            unrecognized_datatypes,
        });
    }
    out
}

fn manifest_includes(doc: &Document, base: &str) -> Vec<String> {
    let mut out = Vec::new();
    for includes in objects(doc, None, MF_INCLUDE) {
        for item in list_items(includes) {
            if let Term::Iri(iri) = item {
                out.push(normalize_manifest_include(iri, base));
            }
        }
    }
    out
}

fn normalize_manifest_include(iri: &str, base: &str) -> String {
    let mut value = resolve_resource(iri, base);
    let no_fragment = value.split('#').next().unwrap_or(&value).to_string();
    if !no_fragment.to_ascii_lowercase().ends_with(".ttl") {
        if !value.ends_with('/') { value.push('/'); }
        value.push_str("manifest.ttl");
    }
    value
}

fn objects<'a>(doc: &'a Document, subject: Option<&Term>, predicate: &str) -> Vec<&'a Term> {
    doc.facts.iter()
        .filter(|t| subject.map_or(true, |s| &t.s == s))
        .filter(|t| matches!(&t.p, Term::Iri(p) if p == predicate))
        .map(|t| &t.o)
        .collect()
}

fn iri_value(term: &Term) -> Option<String> {
    match term { Term::Iri(iri) => Some(iri.clone()), _ => None }
}

fn first_iri_object(doc: &Document, subject: &Term, predicate: &str) -> Option<String> {
    objects(doc, Some(subject), predicate).into_iter().find_map(iri_value)
}

fn first_literal_object(doc: &Document, subject: &Term, predicate: &str) -> Option<String> {
    objects(doc, Some(subject), predicate).into_iter().find_map(|term| match term {
        Term::Literal(lit) => Some(lit.value.clone()),
        Term::Iri(iri) => Some(iri.clone()),
        _ => None,
    })
}

fn first_list_iri_objects(doc: &Document, subject: &Term, predicate: &str) -> Vec<String> {
    objects(doc, Some(subject), predicate).into_iter().next().map(|term| {
        list_items(term).into_iter().filter_map(iri_value).collect()
    }).unwrap_or_default()
}

fn result_object(doc: &Document, subject: &Term) -> (Option<String>, ResultKind) {
    for term in objects(doc, Some(subject), MF_RESULT) {
        match term {
            Term::Iri(iri) => return (Some(iri.clone()), ResultKind::Resource),
            Term::Literal(lit) if lit.value == "false" => return (None, ResultKind::False),
            _ => {}
        }
    }
    (None, ResultKind::None)
}

fn list_items(term: &Term) -> Vec<&Term> {
    match term { Term::List(items) => items.iter().collect(), other => vec![other] }
}

fn term_label(term: &Term) -> String {
    match term {
        Term::Iri(iri) => iri.clone(),
        Term::Blank(id) => format!("_:{id}"),
        Term::Literal(lit) => lit.value.clone(),
        Term::List(items) => format!("list({})", items.len()),
        Term::Formula(_) => "formula".to_string(),
        Term::Var(name) => format!("?{name}"),
    }
}

fn is_supported_type(t: &str) -> bool {
    t.starts_with(RDFT) || t.starts_with(MF)
}

fn case_kind(types: &[String]) -> CaseKind {
    if types.iter().any(|t| t.ends_with("NegativeSyntax") || t.ends_with("NegativeEval")) { CaseKind::NegativeSyntax }
    else if types.iter().any(|t| t.ends_with("PositiveEntailmentTest")) { CaseKind::PositiveEntailment }
    else if types.iter().any(|t| t.ends_with("NegativeEntailmentTest")) { CaseKind::NegativeEntailment }
    else if types.iter().any(|t| t.ends_with("Eval")) { CaseKind::Eval }
    else if types.iter().any(|t| t.ends_with("PositiveSyntax") || t.ends_with("Syntax")) { CaseKind::PositiveSyntax }
    else { CaseKind::Unknown }
}

fn primary_type(types: &[String]) -> Option<String> { types.first().cloned() }

fn format_for(case: &ManifestCase, resource: &str) -> Option<RdfFormat> {
    primary_type(&case.types).and_then(|t| format_from_type(&t)).or_else(|| format_from_resource(resource))
}

fn format_from_type(t: &str) -> Option<RdfFormat> {
    if t.contains("NTriples") { Some(RdfFormat::NTriples) }
    else if t.contains("NQuads") { Some(RdfFormat::NQuads) }
    else if t.contains("Trig") { Some(RdfFormat::Trig) }
    else if t.contains("Turtle") { Some(RdfFormat::Turtle) }
    else { None }
}

fn format_from_resource(resource: &str) -> Option<RdfFormat> {
    let lower = resource.split('#').next().unwrap_or(resource).to_ascii_lowercase();
    if lower.ends_with(".nt") { Some(RdfFormat::NTriples) }
    else if lower.ends_with(".nq") { Some(RdfFormat::NQuads) }
    else if lower.ends_with(".trig") { Some(RdfFormat::Trig) }
    else if lower.ends_with(".ttl") { Some(RdfFormat::Turtle) }
    else { None }
}

fn dataset_quads(doc: &Document) -> Vec<CQuad> {
    let mut builder = DatasetBuilder::default();
    for triple in &doc.facts {
        if let Some((graph, triples)) = named_graph_fact(triple) {
            let graph = builder.term(graph, &CTerm::DefaultGraph);
            for inner in triples { builder.triple(inner, graph.clone()); }
        } else {
            builder.triple(triple, CTerm::DefaultGraph);
        }
    }
    builder.quads
}

fn named_graph_fact(triple: &Triple) -> Option<(&Term, &[Triple])> {
    match (&triple.p, &triple.o) {
        (Term::Iri(p), Term::Formula(triples)) if p == LOG_NAME_OF => Some((&triple.s, triples)),
        _ => None,
    }
}

#[derive(Default)]
struct DatasetBuilder { quads: Vec<CQuad>, blank_counter: usize }

impl DatasetBuilder {
    fn triple(&mut self, triple: &Triple, graph: CTerm) {
        let s = self.term(&triple.s, &graph);
        let p = self.term(&triple.p, &graph);
        let o = self.term(&triple.o, &graph);
        self.quads.push(CQuad { s, p, o, g: graph });
    }

    fn term(&mut self, term: &Term, graph: &CTerm) -> CTerm {
        match term {
            Term::Iri(iri) => CTerm::Iri(iri.clone()),
            Term::Blank(id) => CTerm::Blank(id.clone()),
            Term::Literal(lit) => canonical_literal(lit),
            Term::List(items) => self.list_term(items, graph),
            Term::Formula(triples) if triples.len() == 1 => {
                let triple = &triples[0];
                CTerm::Triple(Box::new(CTripleTerm { s: self.term(&triple.s, graph), p: self.term(&triple.p, graph), o: self.term(&triple.o, graph) }))
            }
            Term::Formula(_) => CTerm::Blank(self.fresh_blank("formula")),
            Term::Var(name) => CTerm::Blank(format!("var_{name}")),
        }
    }

    fn list_term(&mut self, items: &[Term], graph: &CTerm) -> CTerm {
        if items.is_empty() { return CTerm::Iri(RDF_NIL.to_string()); }
        let nodes: Vec<String> = (0..items.len()).map(|_| self.fresh_blank("list")).collect();
        for (idx, item) in items.iter().enumerate() {
            let subject = CTerm::Blank(nodes[idx].clone());
            let value = self.term(item, graph);
            let rest = if idx + 1 < nodes.len() { CTerm::Blank(nodes[idx + 1].clone()) } else { CTerm::Iri(RDF_NIL.to_string()) };
            self.quads.push(CQuad { s: subject.clone(), p: CTerm::Iri(RDF_FIRST.to_string()), o: value, g: graph.clone() });
            self.quads.push(CQuad { s: subject, p: CTerm::Iri(RDF_REST.to_string()), o: rest, g: graph.clone() });
        }
        CTerm::Blank(nodes[0].clone())
    }

    fn fresh_blank(&mut self, prefix: &str) -> String {
        self.blank_counter += 1;
        format!("{prefix}{}", self.blank_counter)
    }
}

fn canonical_literal(lit: &Literal) -> CTerm {
    let language = lit.language.clone().unwrap_or_default().to_ascii_lowercase();
    let datatype = if !language.is_empty() { RDF_LANG_STRING.to_string() } else { lit.datatype.clone().unwrap_or_else(|| XSD_STRING.to_string()) };
    CTerm::Literal { value: lit.value.clone(), datatype, language }
}

fn graphs_isomorphic(actual: &[CQuad], expected: &[CQuad]) -> bool {
    let actual = unique_quads(actual);
    let expected = unique_quads(expected);
    if actual.len() != expected.len() { return false; }
    let actual_blanks = collect_blanks(&actual);
    let expected_blanks = collect_blanks(&expected);
    if actual_blanks.len() != expected_blanks.len() { return false; }
    let expected_set: BTreeSet<String> = expected.iter().map(|q| render_quad(q, &BTreeMap::new(), false)).collect();
    if actual_blanks.is_empty() { return actual.iter().all(|q| expected_set.contains(&render_quad(q, &BTreeMap::new(), false))); }
    let mut order = actual_blanks.iter().cloned().collect::<Vec<_>>();
    order.sort_by_key(|id| std::cmp::Reverse(count_blank_uses(id, &actual)));
    search_blank_mapping(0, &order, &expected_blanks.iter().cloned().collect::<Vec<_>>(), &mut BTreeMap::new(), &mut BTreeSet::new(), &actual, &expected_set)
}

fn unique_quads(quads: &[CQuad]) -> Vec<CQuad> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for q in quads {
        let key = render_quad(q, &BTreeMap::new(), false);
        if seen.insert(key) { out.push(q.clone()); }
    }
    out
}

fn search_blank_mapping(index: usize, order: &[String], candidates: &[String], mapping: &mut BTreeMap<String, String>, used: &mut BTreeSet<String>, actual: &[CQuad], expected_set: &BTreeSet<String>) -> bool {
    if index >= order.len() {
        return actual.iter().all(|q| expected_set.contains(&render_quad(q, mapping, true)));
    }
    let a = &order[index];
    for b in candidates {
        if used.contains(b) { continue; }
        mapping.insert(a.clone(), b.clone());
        used.insert(b.clone());
        if partial_mapping_consistent(mapping, actual, expected_set) && search_blank_mapping(index + 1, order, candidates, mapping, used, actual, expected_set) { return true; }
        used.remove(b);
        mapping.remove(a);
    }
    false
}

fn partial_mapping_consistent(mapping: &BTreeMap<String, String>, actual: &[CQuad], expected_set: &BTreeSet<String>) -> bool {
    for q in actual {
        if let Some(rendered) = render_quad_partial(q, mapping) {
            if !expected_set.contains(&rendered) { return false; }
        }
    }
    true
}

fn count_blank_uses(id: &str, quads: &[CQuad]) -> usize {
    quads.iter().map(|q| render_quad(q, &BTreeMap::new(), false).matches(&format!("_:{id}")).count()).sum()
}

fn collect_blanks(quads: &[CQuad]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for q in quads {
        collect_term_blanks(&q.s, &mut out);
        collect_term_blanks(&q.p, &mut out);
        collect_term_blanks(&q.o, &mut out);
        collect_term_blanks(&q.g, &mut out);
    }
    out
}

fn collect_term_blanks(term: &CTerm, out: &mut BTreeSet<String>) {
    match term {
        CTerm::Blank(id) => { out.insert(id.clone()); }
        CTerm::Triple(t) => { collect_term_blanks(&t.s, out); collect_term_blanks(&t.p, out); collect_term_blanks(&t.o, out); }
        _ => {}
    }
}

fn render_quad(q: &CQuad, mapping: &BTreeMap<String, String>, require_mapping: bool) -> String {
    format!("{} {} {} {}", render_term(&q.s, mapping, require_mapping).unwrap_or_default(), render_term(&q.p, mapping, require_mapping).unwrap_or_default(), render_term(&q.o, mapping, require_mapping).unwrap_or_default(), render_term(&q.g, mapping, require_mapping).unwrap_or_default())
}

fn render_quad_partial(q: &CQuad, mapping: &BTreeMap<String, String>) -> Option<String> {
    Some(format!("{} {} {} {}", render_term(&q.s, mapping, true)?, render_term(&q.p, mapping, true)?, render_term(&q.o, mapping, true)?, render_term(&q.g, mapping, true)?))
}

fn render_term(term: &CTerm, mapping: &BTreeMap<String, String>, require_mapping: bool) -> Option<String> {
    match term {
        CTerm::DefaultGraph => Some("@default".to_string()),
        CTerm::Iri(iri) => Some(format!("<{}>", iri)),
        CTerm::Blank(id) => match mapping.get(id) {
            Some(mapped) => Some(format!("_:{}", mapped)),
            None if require_mapping => None,
            None => Some(format!("_:{}", id)),
        },
        CTerm::Literal { value, datatype, language } => {
            if language.is_empty() { Some(format!("{:?}^^<{}>", value, datatype)) } else { Some(format!("{:?}@{}", value, language)) }
        }
        CTerm::Triple(t) => Some(format!("<<{} {} {}>>", render_term(&t.s, mapping, require_mapping)?, render_term(&t.p, mapping, require_mapping)?, render_term(&t.o, mapping, require_mapping)?)),
    }
}

fn dataset_diff(actual: &[CQuad], expected: &[CQuad]) -> String {
    let actual_set: BTreeSet<_> = actual.iter().map(|q| render_quad(q, &BTreeMap::new(), false)).collect();
    let expected_set: BTreeSet<_> = expected.iter().map(|q| render_quad(q, &BTreeMap::new(), false)).collect();
    let missing: Vec<_> = expected_set.difference(&actual_set).take(5).cloned().collect();
    let extra: Vec<_> = actual_set.difference(&expected_set).take(5).cloned().collect();
    format!("graph mismatch: actual {} quads, expected {} quads; missing {:?}; extra {:?}", actual.len(), expected.len(), missing, extra)
}

#[derive(Debug, Clone)]
struct EntailmentOptions { positive: bool, result_false: bool, regime: String, recognized_datatypes: Vec<String> }
#[derive(Debug, Clone)]
struct EntailmentResult { passed: bool, message: String }

fn evaluate_entailment(input: &[CQuad], expected: &[CQuad], options: EntailmentOptions) -> EntailmentResult {
    let closure = rdfs_closure(input, &options.regime, &options.recognized_datatypes);
    let inconsistent = graph_inconsistent(&closure, &options.recognized_datatypes);
    if options.result_false {
        let passed = if options.positive { inconsistent } else { !inconsistent };
        let message_text = if passed {
            if inconsistent { "input graph is inconsistent as expected" } else { "input graph is consistent as expected" }
        } else if options.positive {
            "expected inconsistency but graph was consistent"
        } else {
            "expected consistency but graph was inconsistent"
        };
        let message = message_text.to_string();
        return EntailmentResult { passed, message };
    }
    let entailed = entails(&closure, expected, &options.recognized_datatypes);
    let passed = if options.positive { entailed } else { !entailed };
    let message = if passed {
        if entailed { "entailed expected graph".to_string() } else { "did not entail expected graph".to_string() }
    } else if options.positive { "expected graph was not entailed".to_string() } else { "negative entailment graph was entailed".to_string() };
    EntailmentResult { passed, message }
}

fn rdfs_closure(input: &[CQuad], regime0: &str, recognized: &[String]) -> Vec<CQuad> {
    let regime = normalize_regime(regime0);
    let mut closure = unique_quads(input);
    let mut seen: BTreeSet<String> = closure.iter().map(quad_key_semantic).collect();
    add_term_semantic_triples(&mut closure, &mut seen, input, &regime, recognized);
    if regime == "rdf" || regime == "rdfs" {
        for ax in rdf_axiomatic_quads() { add_quad(&mut closure, &mut seen, ax); }
        for t in input { add_quad(&mut closure, &mut seen, q(t.p.clone(), iri(RDF_TYPE), iri(RDF_PROPERTY))); }
    }
    if regime == "rdfs" {
        for ax in rdfs_axiomatic_quads(recognized) { add_quad(&mut closure, &mut seen, ax); }
        for i in 1..=10 { add_quad(&mut closure, &mut seen, q(iri(&format!("{RDF}_{i}")), iri(RDF_TYPE), iri(RDFS_CONTAINER_MEMBERSHIP_PROPERTY))); }
    }
    if regime != "rdfs" { return closure; }
    let mut changed = true;
    let mut iterations = 0;
    while changed && iterations < 1000 {
        iterations += 1;
        changed = false;
        let snapshot = closure.clone();
        for t in &snapshot {
            if matches!(t.p, CTerm::Iri(_)) { changed |= add_quad(&mut closure, &mut seen, q(t.p.clone(), iri(RDF_TYPE), iri(RDF_PROPERTY))); }
            if let CTerm::Iri(p) = &t.p {
                if is_container_membership_property(p) { changed |= add_quad(&mut closure, &mut seen, q(t.p.clone(), iri(RDFS_SUBPROPERTY_OF), iri(RDFS_MEMBER))); }
            }
        }
        let sub_props: Vec<CQuad> = closure.iter().filter(|t| is_iri(&t.p, RDFS_SUBPROPERTY_OF)).cloned().collect();
        let sub_classes: Vec<CQuad> = closure.iter().filter(|t| is_iri(&t.p, RDFS_SUBCLASS_OF)).cloned().collect();
        let domains: Vec<CQuad> = closure.iter().filter(|t| is_iri(&t.p, RDFS_DOMAIN)).cloned().collect();
        let ranges: Vec<CQuad> = closure.iter().filter(|t| is_iri(&t.p, RDFS_RANGE)).cloned().collect();
        for sp in &sub_props { changed |= add_quad(&mut closure, &mut seen, q(sp.s.clone(), iri(RDF_TYPE), iri(RDF_PROPERTY))); changed |= add_quad(&mut closure, &mut seen, q(sp.o.clone(), iri(RDF_TYPE), iri(RDF_PROPERTY))); }
        for sc in &sub_classes { changed |= add_quad(&mut closure, &mut seen, q(sc.s.clone(), iri(RDF_TYPE), iri(RDFS_CLASS))); changed |= add_quad(&mut closure, &mut seen, q(sc.o.clone(), iri(RDF_TYPE), iri(RDFS_CLASS))); }
        for d in &domains { changed |= add_quad(&mut closure, &mut seen, q(d.s.clone(), iri(RDF_TYPE), iri(RDF_PROPERTY))); changed |= add_quad(&mut closure, &mut seen, q(d.o.clone(), iri(RDF_TYPE), iri(RDFS_CLASS))); }
        for r in &ranges { changed |= add_quad(&mut closure, &mut seen, q(r.s.clone(), iri(RDF_TYPE), iri(RDF_PROPERTY))); changed |= add_quad(&mut closure, &mut seen, q(r.o.clone(), iri(RDF_TYPE), iri(RDFS_CLASS))); }
        for t in closure.clone() {
            if is_iri(&t.p, RDF_TYPE) && is_iri(&t.o, RDF_PROPERTY) { changed |= add_quad(&mut closure, &mut seen, q(t.s.clone(), iri(RDFS_SUBPROPERTY_OF), t.s.clone())); }
            if is_iri(&t.p, RDF_TYPE) && is_iri(&t.o, RDFS_CLASS) { changed |= add_quad(&mut closure, &mut seen, q(t.s.clone(), iri(RDFS_SUBCLASS_OF), t.s.clone())); }
            if is_iri(&t.p, RDF_TYPE) && is_iri(&t.o, RDFS_DATATYPE) { changed |= add_quad(&mut closure, &mut seen, q(t.s.clone(), iri(RDFS_SUBCLASS_OF), iri(RDFS_LITERAL))); }
        }
        for a in &sub_props {
            for b in &sub_props { if terms_equal(&a.o, &b.s, recognized) { changed |= add_quad(&mut closure, &mut seen, q(a.s.clone(), iri(RDFS_SUBPROPERTY_OF), b.o.clone())); } }
            for t in &snapshot { if terms_equal(&t.p, &a.s, recognized) { changed |= add_quad(&mut closure, &mut seen, q(t.s.clone(), a.o.clone(), t.o.clone())); } }
        }
        for a in &sub_classes {
            for b in &sub_classes { if terms_equal(&a.o, &b.s, recognized) { changed |= add_quad(&mut closure, &mut seen, q(a.s.clone(), iri(RDFS_SUBCLASS_OF), b.o.clone())); } }
            for t in &snapshot { if is_iri(&t.p, RDF_TYPE) && terms_equal(&t.o, &a.s, recognized) { changed |= add_quad(&mut closure, &mut seen, q(t.s.clone(), iri(RDF_TYPE), a.o.clone())); } }
        }
        for d in &domains { for t in &snapshot { if terms_equal(&t.p, &d.s, recognized) { changed |= add_quad(&mut closure, &mut seen, q(t.s.clone(), iri(RDF_TYPE), d.o.clone())); } } }
        for r in &ranges { for t in &snapshot { if terms_equal(&t.p, &r.s, recognized) { changed |= add_quad(&mut closure, &mut seen, q(t.o.clone(), iri(RDF_TYPE), r.o.clone())); } } }
    }
    closure
}

fn add_term_semantic_triples(closure: &mut Vec<CQuad>, seen: &mut BTreeSet<String>, triples: &[CQuad], regime: &str, recognized: &[String]) {
    for t in triples {
        for term in [&t.s, &t.p, &t.o, &t.g] { add_for_term(term, closure, seen, regime, recognized); }
        if is_iri(&t.p, RDF_REIFIES) {
            add_quad(closure, seen, q(t.s.clone(), iri(RDF_TYPE), iri(RDFS_PROPOSITION)));
            add_quad(closure, seen, q(t.o.clone(), iri(RDF_TYPE), iri(RDFS_PROPOSITION)));
            add_for_term(&t.o, closure, seen, regime, recognized);
        }
    }
}

fn add_for_term(term: &CTerm, closure: &mut Vec<CQuad>, seen: &mut BTreeSet<String>, regime: &str, _recognized: &[String]) {
    match term {
        CTerm::Literal { datatype, .. } => {
            add_quad(closure, seen, q(term.clone(), iri(RDF_TYPE), iri(datatype)));
            if regime == "rdfs" { add_quad(closure, seen, q(term.clone(), iri(RDF_TYPE), iri(RDFS_LITERAL))); }
        }
        CTerm::Triple(t) => {
            add_quad(closure, seen, q(term.clone(), iri(RDF_TYPE), iri(RDF_TRIPLE_TERM)));
            add_quad(closure, seen, q(term.clone(), iri(RDF_TYPE), iri(RDFS_PROPOSITION)));
            add_for_term(&t.s, closure, seen, regime, _recognized);
            add_for_term(&t.p, closure, seen, regime, _recognized);
            add_for_term(&t.o, closure, seen, regime, _recognized);
        }
        _ => {}
    }
}

fn rdf_axiomatic_quads() -> Vec<CQuad> {
    let properties = vec![
        RDF_TYPE.to_string(), format!("{RDF}subject"), format!("{RDF}predicate"), format!("{RDF}object"),
        RDF_FIRST.to_string(), RDF_REST.to_string(), format!("{RDF}value"), RDFS_SUBCLASS_OF.to_string(),
        RDFS_SUBPROPERTY_OF.to_string(), RDFS_DOMAIN.to_string(), RDFS_RANGE.to_string(), format!("{RDFS}label"),
        format!("{RDFS}comment"), format!("{RDFS}seeAlso"), format!("{RDFS}isDefinedBy"), RDFS_MEMBER.to_string(),
    ];
    let mut out = Vec::new();
    for p in properties { out.push(q(iri(&p), iri(RDF_TYPE), iri(RDF_PROPERTY))); }
    for i in 1..=10 { out.push(q(iri(&format!("{RDF}_{i}")), iri(RDF_TYPE), iri(RDF_PROPERTY))); }
    out
}

fn rdfs_axiomatic_quads(recognized: &[String]) -> Vec<CQuad> {
    let mut out = rdf_axiomatic_quads();
    let classes = vec![
        RDFS_RESOURCE.to_string(), RDFS_CLASS.to_string(), RDFS_LITERAL.to_string(), RDFS_DATATYPE.to_string(),
        RDF_PROPERTY.to_string(), format!("{RDF}List"), RDFS_CONTAINER_MEMBERSHIP_PROPERTY.to_string(), RDFS_PROPOSITION.to_string(),
    ];
    for c in classes { out.push(q(iri(&c), iri(RDF_TYPE), iri(RDFS_CLASS))); }
    out.push(q(iri(RDFS_DATATYPE), iri(RDFS_SUBCLASS_OF), iri(RDFS_CLASS)));
    for dt in [RDF_LANG_STRING, RDF_DIR_LANG_STRING, RDF_XML_LITERAL, RDF_HTML, RDF_JSON] { out.push(q(iri(dt), iri(RDF_TYPE), iri(RDFS_DATATYPE))); }
    for dt in recognized { out.push(q(iri(dt), iri(RDF_TYPE), iri(RDFS_DATATYPE))); }
    for (s, p, o) in [
        (RDFS_SUBCLASS_OF, RDFS_DOMAIN, RDFS_CLASS), (RDFS_SUBCLASS_OF, RDFS_RANGE, RDFS_CLASS),
        (RDFS_SUBPROPERTY_OF, RDFS_DOMAIN, RDF_PROPERTY), (RDFS_SUBPROPERTY_OF, RDFS_RANGE, RDF_PROPERTY),
        (RDFS_DOMAIN, RDFS_DOMAIN, RDF_PROPERTY), (RDFS_DOMAIN, RDFS_RANGE, RDFS_CLASS),
        (RDFS_RANGE, RDFS_DOMAIN, RDF_PROPERTY), (RDFS_RANGE, RDFS_RANGE, RDFS_CLASS),
        (RDFS_MEMBER, RDF_TYPE, RDF_PROPERTY), (RDFS_MEMBER, RDFS_DOMAIN, RDFS_RESOURCE), (RDFS_MEMBER, RDFS_RANGE, RDFS_RESOURCE),
        (RDF_REIFIES, RDF_TYPE, RDF_PROPERTY), (RDF_REIFIES, RDFS_DOMAIN, RDFS_RESOURCE), (RDF_REIFIES, RDFS_RANGE, RDFS_PROPOSITION),
        (RDF_TRIPLE_TERM, RDFS_SUBCLASS_OF, RDFS_PROPOSITION), (RDF_PROPOSITION, RDFS_SUBCLASS_OF, RDFS_PROPOSITION),
        (RDFS_CONTAINER_MEMBERSHIP_PROPERTY, RDFS_SUBCLASS_OF, RDF_PROPERTY),
    ] { out.push(q(iri(s), iri(p), iri(o))); }
    for i in 1..=10 { out.push(q(iri(&format!("{RDF}_{i}")), iri(RDFS_SUBPROPERTY_OF), iri(RDFS_MEMBER))); }
    out
}

fn add_quad(closure: &mut Vec<CQuad>, seen: &mut BTreeSet<String>, quad: CQuad) -> bool {
    let key = quad_key_semantic(&quad);
    if seen.insert(key) { closure.push(quad); true } else { false }
}

fn q(s: CTerm, p: CTerm, o: CTerm) -> CQuad { CQuad { s, p, o, g: CTerm::DefaultGraph } }
fn iri(value: &str) -> CTerm { CTerm::Iri(value.to_string()) }
fn is_iri(term: &CTerm, value: &str) -> bool { matches!(term, CTerm::Iri(iri) if iri == value) }
fn is_container_membership_property(value: &str) -> bool { value.strip_prefix(RDF).is_some_and(|local| local.starts_with('_') && local[1..].chars().all(|c| c.is_ascii_digit()) && !local[1..].is_empty()) }

fn normalize_regime(regime: &str) -> String {
    let value = regime.trim().to_ascii_lowercase();
    if value == "rdfs" || value == "rdfs-entailment" { "rdfs".to_string() }
    else if value == "rdf" || value == "rdf-entailment" { "rdf".to_string() }
    else { "simple".to_string() }
}

fn normalize_datatype(dt: &str) -> &str { if dt == XSD_INT { XSD_INTEGER } else { dt } }

fn datatype_kind(dt: &str) -> Option<&'static str> {
    match normalize_datatype(dt) {
        XSD_INTEGER | XSD_DECIMAL => Some("decimal"),
        XSD_FLOAT => Some("float"),
        XSD_DOUBLE => Some("double"),
        XSD_STRING => Some("string"),
        RDF_LANG_STRING => Some("langString"),
        RDF_DIR_LANG_STRING => Some("dirLangString"),
        RDF_XML_LITERAL => Some("xml"),
        RDF_JSON => Some("json"),
        _ => None,
    }
}

fn datatype_compatible(a: &str, b: &str) -> bool {
    match (datatype_kind(a), datatype_kind(b)) {
        (Some(ak), Some(bk)) => ak == bk || (ak == "decimal" && bk == "decimal"),
        _ => true,
    }
}

fn graph_inconsistent(quads: &[CQuad], recognized0: &[String]) -> bool {
    let mut recognized: BTreeSet<String> = recognized0.iter().cloned().collect();
    for dt in recognized0 { recognized.insert(normalize_datatype(dt).to_string()); }
    for q in quads {
        for term in collect_terms_deep(q) {
            if let CTerm::Literal { value, datatype, .. } = term {
                if (recognized.contains(datatype) || recognized.contains(normalize_datatype(datatype))) && !valid_lexical_literal(value, datatype) { return true; }
            }
        }
    }

    let mut type_rows: BTreeMap<CTerm, Vec<String>> = BTreeMap::new();
    for q in quads {
        if is_iri(&q.p, RDF_TYPE) {
            if let CTerm::Iri(dt) = &q.o {
                type_rows.entry(q.s.clone()).or_default().push(dt.clone());
            }
        }
    }
    for (term, types) in type_rows {
        if let CTerm::Literal { value, datatype, .. } = &term {
            for ty in &types {
                if !recognized.contains(ty) && !recognized.contains(normalize_datatype(ty)) { continue; }
                if !datatype_compatible(datatype, ty) { return true; }
                if !valid_lexical_literal(value, ty) { return true; }
            }
        }
        for i in 0..types.len() {
            for j in (i + 1)..types.len() {
                let a = &types[i];
                let b = &types[j];
                if (recognized.contains(a) || recognized.contains(normalize_datatype(a))) && (recognized.contains(b) || recognized.contains(normalize_datatype(b))) && !datatype_compatible(a, b) { return true; }
            }
        }
    }

    for q in quads {
        if is_iri(&q.p, RDFS_SUBCLASS_OF) {
            if let (CTerm::Iri(a), CTerm::Iri(b)) = (&q.s, &q.o) {
                if (recognized.contains(a) || recognized.contains(normalize_datatype(a))) && (recognized.contains(b) || recognized.contains(normalize_datatype(b))) && !datatype_compatible(a, b) { return true; }
            }
        }
    }
    false
}

fn collect_terms_deep(q: &CQuad) -> Vec<&CTerm> {
    fn add<'a>(term: &'a CTerm, out: &mut Vec<&'a CTerm>) {
        out.push(term);
        if let CTerm::Triple(t) = term {
            add(&t.s, out);
            add(&t.p, out);
            add(&t.o, out);
        }
    }
    let mut out = Vec::new();
    add(&q.s, &mut out);
    add(&q.p, &mut out);
    add(&q.o, &mut out);
    add(&q.g, &mut out);
    out
}

fn valid_lexical_literal(lex: &str, dt: &str) -> bool {
    match normalize_datatype(dt) {
        XSD_INTEGER => lex.parse::<i128>().is_ok(),
        XSD_DECIMAL => is_decimal_lexical(lex),
        XSD_BOOLEAN => matches!(lex, "true" | "false" | "1" | "0"),
        XSD_FLOAT | XSD_DOUBLE => is_float_lexical(lex),
        RDF_JSON => json_maybe_valid(lex),
        RDF_XML_LITERAL => xml_maybe_valid(lex),
        _ => true,
    }
}

fn is_decimal_lexical(s: &str) -> bool {
    let s = s.strip_prefix('+').or_else(|| s.strip_prefix('-')).unwrap_or(s);
    if s.is_empty() { return false; }
    if let Some((a, b)) = s.split_once('.') { (!a.is_empty() || !b.is_empty()) && a.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit()) } else { s.chars().all(|c| c.is_ascii_digit()) }
}
fn is_float_lexical(s: &str) -> bool { matches!(s, "INF" | "-INF" | "NaN") || s.parse::<f64>().is_ok() }
fn json_maybe_valid(s: &str) -> bool { serde_json::from_str::<JsonValue>(s).is_ok() || json_number_canonical(s).is_some() }
fn xml_maybe_valid(s: &str) -> bool {
    if s.contains('&') && !(s.contains("&amp;") || s.contains("&lt;") || s.contains("&gt;") || s.contains("&quot;") || s.contains("&apos;")) { return false; }
    let mut rest = s;
    while let Some(idx) = rest.find('<') {
        rest = &rest[idx + 1..];
        let Some(first) = rest.chars().next() else { return false; };
        if !(first.is_ascii_alphabetic() || matches!(first, '/' | '?' | '!')) { return false; }
        let Some(end) = rest.find('>') else { return false; };
        rest = &rest[end + 1..];
    }
    true
}

fn entails(closure: &[CQuad], expected: &[CQuad], recognized: &[String]) -> bool {
    let expected = unique_quads(expected);
    let mut order = expected;
    order.sort_by_key(|p| candidate_count(p, closure, recognized));
    entails_search(0, &order, closure, &mut BTreeMap::new(), recognized)
}

fn entails_search(index: usize, expected: &[CQuad], closure: &[CQuad], binding: &mut BTreeMap<String, CTerm>, recognized: &[String]) -> bool {
    if index >= expected.len() { return true; }
    let pat = &expected[index];
    for cand in closure {
        let saved = binding.clone();
        if match_expected_quad(pat, cand, binding, recognized) && entails_search(index + 1, expected, closure, binding, recognized) { return true; }
        *binding = saved;
    }
    false
}

fn candidate_count(pattern: &CQuad, closure: &[CQuad], recognized: &[String]) -> usize {
    let mut count = 0;
    for cand in closure {
        let mut binding = BTreeMap::new();
        if match_expected_quad(pattern, cand, &mut binding, recognized) { count += 1; }
    }
    if count == 0 { closure.len() + 1 } else { count }
}

fn match_expected_quad(expected: &CQuad, actual: &CQuad, binding: &mut BTreeMap<String, CTerm>, recognized: &[String]) -> bool {
    match_expected_term(&expected.s, &actual.s, binding, recognized) && match_expected_term(&expected.p, &actual.p, binding, recognized) && match_expected_term(&expected.o, &actual.o, binding, recognized) && match_expected_term(&expected.g, &actual.g, binding, recognized)
}

fn match_expected_term(expected: &CTerm, actual: &CTerm, binding: &mut BTreeMap<String, CTerm>, recognized: &[String]) -> bool {
    match expected {
        CTerm::Blank(id) => {
            if let Some(bound) = binding.get(id) { terms_equal(bound, actual, recognized) } else { binding.insert(id.clone(), actual.clone()); true }
        }
        CTerm::Triple(t) => match actual {
            CTerm::Triple(a) => match_expected_term(&t.s, &a.s, binding, recognized) && match_expected_term(&t.p, &a.p, binding, recognized) && match_expected_term(&t.o, &a.o, binding, recognized),
            _ => false,
        },
        _ => terms_equal(expected, actual, recognized),
    }
}

fn terms_equal(a: &CTerm, b: &CTerm, recognized: &[String]) -> bool {
    match (a, b) {
        (CTerm::DefaultGraph, CTerm::DefaultGraph) => true,
        (CTerm::Iri(a), CTerm::Iri(b)) => a == b,
        (CTerm::Blank(a), CTerm::Blank(b)) => a == b,
        (CTerm::Literal { value: av, datatype: ad, language: al }, CTerm::Literal { value: bv, datatype: bd, language: bl }) => {
            if al != bl { return false; }
            let ak = datatype_kind(ad);
            let bk = datatype_kind(bd);
            if ak.is_some() && bk.is_some() && !(ak == bk || (ak == Some("decimal") && bk == Some("decimal"))) { return false; }
            if ak.is_none() && bk.is_none() && ad != bd { return false; }
            canonical_literal_value(av, ad, recognized) == canonical_literal_value(bv, bd, recognized)
        }
        (CTerm::Triple(a), CTerm::Triple(b)) => terms_equal(&a.s, &b.s, recognized) && terms_equal(&a.p, &b.p, recognized) && terms_equal(&a.o, &b.o, recognized),
        _ => false,
    }
}

fn canonical_literal_value(value: &str, datatype: &str, recognized0: &[String]) -> String {
    let mut recognized: BTreeSet<String> = recognized0.iter().cloned().collect();
    for dt in recognized0 { recognized.insert(normalize_datatype(dt).to_string()); }
    let ndt = normalize_datatype(datatype);
    if !recognized.contains(datatype) && !recognized.contains(ndt) { return format!("raw:{value:?}^^{datatype}"); }
    match ndt {
        XSD_INTEGER | XSD_DECIMAL => decimal_canonical(value).unwrap_or_else(|| format!("invalid:{datatype}:{value}")),
        XSD_BOOLEAN => format!("boolean:{}", value == "true" || value == "1"),
        XSD_FLOAT => parse_special_float(value).map(|n| format!("float:{n:?}")).unwrap_or_else(|| format!("number:{value}")),
        XSD_DOUBLE => parse_special_double(value).map(|n| format!("double:{n:?}")).unwrap_or_else(|| format!("number:{value}")),
        XSD_STRING => format!("string:{value:?}"),
        RDF_JSON => json_canonical_text(value).unwrap_or_else(|| format!("raw:{value:?}^^{datatype}")),
        _ => format!("raw:{value:?}^^{datatype}"),
    }
}



fn json_canonical_text(value: &str) -> Option<String> {
    if let Ok(v) = serde_json::from_str::<JsonValue>(value) {
        return Some(format!("json:{}", json_canonical_value(&v)));
    }
    json_number_canonical(value).map(|n| format!("json:number:{n}"))
}

fn json_number_canonical(value: &str) -> Option<String> {
    let s = value.trim();
    if s.is_empty() || s != value { return None; }
    let bytes = s.as_bytes();
    let mut i = 0usize;
    if bytes.get(i) == Some(&b'-') { i += 1; }
    if i >= bytes.len() { return None; }
    if bytes[i] == b'0' {
        i += 1;
        if i < bytes.len() && bytes[i].is_ascii_digit() { return None; }
    } else if bytes[i].is_ascii_digit() && bytes[i] != b'0' {
        while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
    } else { return None; }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
        if i == start { return None; }
    }
    if i < bytes.len() && matches!(bytes[i], b'e' | b'E') {
        i += 1;
        if i < bytes.len() && matches!(bytes[i], b'+' | b'-') { i += 1; }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
        if i == start { return None; }
    }
    if i != bytes.len() { return None; }
    s.parse::<f64>().ok().map(number_canonical)
}

fn json_canonical_value(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(v) => format!("boolean:{v}"),
        JsonValue::Number(n) => {
            if let Some(f) = n.as_f64() { format!("number:{}", number_canonical(f)) }
            else { format!("number:{}", n) }
        }
        JsonValue::String(s) => format!("string:{s:?}"),
        JsonValue::Array(items) => format!("array:[{}]", items.iter().map(json_canonical_value).collect::<Vec<_>>().join(",")),
        JsonValue::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();
            format!("object:{{{}}}", keys.into_iter().map(|k| format!("{k:?}:{}", json_canonical_value(&map[k]))).collect::<Vec<_>>().join(","))
        }
    }
}

fn number_canonical(value: f64) -> String {
    if value.is_nan() { "NaN".to_string() }
    else if value == f64::INFINITY { "Infinity".to_string() }
    else if value == f64::NEG_INFINITY { "-Infinity".to_string() }
    else if value == 0.0 && value.is_sign_negative() { "-0".to_string() }
    else if value == 0.0 { "+0".to_string() }
    else { value.to_string() }
}

fn parse_special_float(value: &str) -> Option<f32> {
    match value {
        "INF" => Some(f32::INFINITY),
        "-INF" => Some(f32::NEG_INFINITY),
        "NaN" => Some(f32::NAN),
        _ => value.parse::<f32>().ok(),
    }
}

fn parse_special_double(value: &str) -> Option<f64> {
    match value {
        "INF" => Some(f64::INFINITY),
        "-INF" => Some(f64::NEG_INFINITY),
        "NaN" => Some(f64::NAN),
        _ => value.parse::<f64>().ok(),
    }
}

fn decimal_canonical(value: &str) -> Option<String> {
    if !is_decimal_lexical(value) { return None; }
    let mut s = value.to_string();
    let mut sign = "";
    if s.starts_with('+') { s.remove(0); }
    if s.starts_with('-') { sign = "-"; s.remove(0); }
    let (mut int_part, mut frac_part) = match s.split_once('.') { Some((a, b)) => (a.to_string(), b.to_string()), None => (s, String::new()) };
    if int_part.is_empty() { int_part.push('0'); }
    while int_part.len() > 1 && int_part.starts_with('0') { int_part.remove(0); }
    while frac_part.ends_with('0') { frac_part.pop(); }
    if int_part == "0" && frac_part.is_empty() { sign = ""; }
    Some(if frac_part.is_empty() { format!("decimal:{sign}{int_part}") } else { format!("decimal:{sign}{int_part}.{frac_part}") })
}

fn quad_key_semantic(q: &CQuad) -> String { render_quad(q, &BTreeMap::new(), false) }

fn normalize_resource(resource: &str) -> String {
    if is_url(resource) { return resource.to_string(); }
    let path = PathBuf::from(resource);
    let path = if path.is_dir() { path.join("manifest.ttl") } else { path };
    path.to_string_lossy().to_string()
}

fn fetch_url(resource: &str) -> Result<String, String> {
    ureq::get(resource)
        .call()
        .map_err(|err| err.to_string())?
        .into_string()
        .map_err(|err| err.to_string())
}

fn github_raw_fallback(resource: &str) -> Option<String> {
    let prefix = "https://w3c.github.io/rdf-tests/";
    resource.strip_prefix(prefix).map(|rest| format!("https://raw.githubusercontent.com/w3c/rdf-tests/main/{rest}"))
}

fn is_url(value: &str) -> bool { value.starts_with("http://") || value.starts_with("https://") }

fn env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn w3c_rdf_relative_path(resource: &str) -> Option<PathBuf> {
    let github_pages = "https://w3c.github.io/rdf-tests/";
    let raw_github = "https://raw.githubusercontent.com/w3c/rdf-tests/main/";
    resource
        .strip_prefix(github_pages)
        .or_else(|| resource.strip_prefix(raw_github))
        .map(|rest| PathBuf::from(rest.trim_start_matches('/')))
}

fn resolve_resource(reference: &str, base: &str) -> String {
    if is_url(reference) { return reference.to_string(); }
    if reference.starts_with("file:") { return reference.trim_start_matches("file://").to_string(); }
    if is_url(base) { return join_url(base, reference); }
    let base_path = Path::new(base).parent().unwrap_or_else(|| Path::new("."));
    base_path.join(reference).to_string_lossy().to_string()
}

fn join_url(base: &str, reference: &str) -> String {
    if reference.starts_with('/') {
        if let Some((scheme_host, _)) = split_url_origin(base) { return format!("{scheme_host}{reference}"); }
    }
    let dir = match base.rfind('/') { Some(idx) => &base[..idx + 1], None => base };
    remove_url_dot_segments(&format!("{dir}{reference}"))
}

fn split_url_origin(url: &str) -> Option<(String, String)> {
    let scheme_end = url.find("://")? + 3;
    let rest = &url[scheme_end..];
    let slash = rest.find('/').unwrap_or(rest.len());
    Some((url[..scheme_end + slash].to_string(), rest[slash..].to_string()))
}

fn remove_url_dot_segments(url: &str) -> String {
    let Some((origin, path)) = split_url_origin(url) else { return url.to_string(); };
    let mut parts = Vec::new();
    for seg in path.split('/') {
        match seg { "" | "." => {}, ".." => { parts.pop(); }, other => parts.push(other) }
    }
    format!("{origin}/{}", parts.join("/"))
}

fn total_counts(runs: &[ManifestRun]) -> Counts {
    let mut c = Counts::default();
    for run in runs { for item in &run.results { c.total += 1; match item.status { Status::Pass => c.pass += 1, Status::Fail => c.fail += 1, Status::Skip => c.skip += 1 } } }
    c
}

fn should_print_case_progress(item: &CaseResult, index: usize, verbose: bool) -> bool {
    verbose || !matches!(item.status, Status::Pass) || index == 0 || (index + 1) % 100 == 0
}

fn format_progress_line(item: &CaseResult, index: usize) -> String {
    let tag = match item.status { Status::Pass => "OK", Status::Fail => "FAIL", Status::Skip => "SKIP" };
    let ty = primary_type(&item.case.types).unwrap_or_else(|| "unknown".to_string());
    let mut line = format!("{:03} {} {} {} ({} ms)", index + 1, tag, compact_type(&ty), item.case.name, item.duration_ms);
    if item.status != Status::Pass { line.push_str(&format!("\n    {}", item.message)); }
    line
}

fn format_manifest_summary(run: &ManifestRun) -> String {
    let counts = total_counts(std::slice::from_ref(run));
    format!("{} {}/{} tests passed{} ({} ms)", if counts.fail == 0 { "OK" } else { "FAIL" }, counts.pass, counts.total, if counts.skip > 0 { format!(", {} skipped", counts.skip) } else { String::new() }, run.duration_ms)
}

fn compact_type(t: &str) -> String {
    t.strip_prefix(RDFT).or_else(|| t.strip_prefix(MF)).unwrap_or(t).to_string()
}

fn rdf_manifests_to_earl(runs: &[ManifestRun]) -> String {
    let asserted_by = "<https://github.com/eyereasoner/euleron>";
    let mut lines = vec![
        "@prefix earl: <http://www.w3.org/ns/earl#> .".to_string(),
        "@prefix doap: <http://usefulinc.com/ns/doap#> .".to_string(),
        "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .".to_string(),
        String::new(),
        format!("{asserted_by} a earl:Software, doap:Project ;"),
        "  doap:name \"Euleron\" .".to_string(),
        String::new(),
    ];
    for run in runs {
        for item in &run.results {
            let outcome = match item.status { Status::Pass => "earl:passed", Status::Fail => "earl:failed", Status::Skip => "earl:untested" };
            let test_uri = item.case.action.clone().unwrap_or_else(|| term_label(&item.case.id));
            lines.push("[] a earl:Assertion ;".to_string());
            lines.push(format!("  earl:assertedBy {asserted_by} ;"));
            lines.push(format!("  earl:subject {asserted_by} ;"));
            lines.push(format!("  earl:test <{}> ;", escape_iri(&test_uri)));
            lines.push("  earl:result [".to_string());
            lines.push("    a earl:TestResult ;".to_string());
            lines.push(format!("    earl:outcome {outcome} ;"));
            lines.push(format!("    earl:info {} ;", turtle_string(&item.message)));
            lines.push("  ] .".to_string());
            lines.push(String::new());
        }
    }
    lines.join("\n")
}

fn escape_iri(iri: &str) -> String { iri.replace('<', "").replace('>', "") }
fn turtle_string(s: &str) -> String { format!("{:?}", s) }

fn print_json_summary(runs: &[ManifestRun], counts: &Counts, duration_ms: u128) {
    println!("{{");
    println!("  \"ok\": {},", counts.fail == 0 && counts.total > 0);
    println!("  \"manifestCount\": {},", runs.len());
    println!("  \"counts\": {{ \"total\": {}, \"pass\": {}, \"fail\": {}, \"skip\": {} }},", counts.total, counts.pass, counts.fail, counts.skip);
    println!("  \"durationMs\": {},", duration_ms);
    println!("  \"manifests\": [");
    for (i, run) in runs.iter().enumerate() {
        let c = total_counts(std::slice::from_ref(run));
        println!("    {{ \"source\": {}, \"manifestFiles\": {}, \"total\": {}, \"pass\": {}, \"fail\": {}, \"skip\": {} }}{}", turtle_string(&run.source), run.manifests_loaded.len(), c.total, c.pass, c.fail, c.skip, if i + 1 == runs.len() { "" } else { "," });
    }
    println!("  ]");
    println!("}}");
}
