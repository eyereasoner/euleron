#[path = "w3c_rdf/runner.rs"]
mod runner;

use std::env;
use std::ffi::OsString;
use std::process;
use std::time::Instant;

struct ManifestSpec {
    label: &'static str,
    url: &'static str,
    expected: usize,
}

const MANIFESTS: &[ManifestSpec] = &[
    ManifestSpec { label: "w3c_rdf_01_rdf11_n_triples_70", url: "https://w3c.github.io/rdf-tests/rdf/rdf11/rdf-n-triples/manifest.ttl", expected: 70 },
    ManifestSpec { label: "w3c_rdf_02_rdf12_n_triples_29", url: "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-n-triples/syntax/manifest.ttl", expected: 29 },
    ManifestSpec { label: "w3c_rdf_03_rdf11_n_quads_87", url: "https://w3c.github.io/rdf-tests/rdf/rdf11/rdf-n-quads/manifest.ttl", expected: 87 },
    ManifestSpec { label: "w3c_rdf_04_rdf12_n_quads_27", url: "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-n-quads/syntax/manifest.ttl", expected: 27 },
    ManifestSpec { label: "w3c_rdf_05_rdf11_semantics_48", url: "https://w3c.github.io/rdf-tests/rdf/rdf11/rdf-mt/manifest.ttl", expected: 48 },
    ManifestSpec { label: "w3c_rdf_06_rdf12_semantics_77", url: "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-semantics/manifest.ttl", expected: 77 },
    ManifestSpec { label: "w3c_rdf_07_rdf11_turtle_313", url: "https://w3c.github.io/rdf-tests/rdf/rdf11/rdf-turtle/manifest.ttl", expected: 313 },
    ManifestSpec { label: "w3c_rdf_08_rdf12_turtle_eval_29", url: "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-turtle/eval/manifest.ttl", expected: 29 },
    ManifestSpec { label: "w3c_rdf_09_rdf12_turtle_syntax_74", url: "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-turtle/syntax/manifest.ttl", expected: 74 },
    ManifestSpec { label: "w3c_rdf_10_rdf11_trig_356", url: "https://w3c.github.io/rdf-tests/rdf/rdf11/rdf-trig/manifest.ttl", expected: 356 },
    ManifestSpec { label: "w3c_rdf_11_rdf12_trig_eval_25", url: "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-trig/eval/manifest.ttl", expected: 25 },
    ManifestSpec { label: "w3c_rdf_12_rdf12_trig_syntax_35", url: "https://w3c.github.io/rdf-tests/rdf/rdf12/rdf-trig/syntax/manifest.ttl", expected: 35 },
];

fn main() {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let config = HarnessConfig::from_args(&args);
    if config.help {
        print_help();
        return;
    }
    if config.list {
        for test in MANIFESTS.iter().map(|m| m.label).chain(std::iter::once("w3c_rdf_13_all_manifests_1170_earl_report")) {
            println!("{test}: test");
        }
        return;
    }

    let selected = MANIFESTS
        .iter()
        .filter(|manifest| config.matches(manifest.label))
        .collect::<Vec<_>>();
    let run_aggregate = config.matches("w3c_rdf_13_all_manifests_1170_earl_report");
    let total_tests = selected.len() + usize::from(run_aggregate);

    println!("running {total_tests} test{}", if total_tests == 1 { "" } else { "s" });
    if total_tests == 0 {
        println!("\ntest result: {}. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s", colour(&config, "ok", Colour::Green));
        return;
    }

    let started = Instant::now();
    let quiet_guard = QuietGuard::enable_unless_verbose(config.verbose);
    let mut passed = 0usize;
    let mut failed = 0usize;

    for manifest in selected {
        if runner::refresh_requested() {
            print_result(&config, manifest.label, TestOutcome::Ok, "refresh delegated to aggregate test");
            passed += 1;
            continue;
        }
        match runner::run_manifest_suite_for_test(manifest.label, manifest.url, manifest.expected) {
            Ok(()) => {
                print_result(&config, manifest.label, TestOutcome::Ok, &format!("{} tests", manifest.expected));
                passed += 1;
            }
            Err(err) => {
                print_result(&config, manifest.label, TestOutcome::Failed, &err);
                failed += 1;
            }
        }
    }

    if run_aggregate {
        match runner::run_default_suite() {
            Ok(()) => {
                print_result(&config, "w3c_rdf_13_all_manifests_1170_earl_report", TestOutcome::Ok, "1170 tests + EARL report");
                passed += 1;
            }
            Err(err) => {
                print_result(&config, "w3c_rdf_13_all_manifests_1170_earl_report", TestOutcome::Failed, &err);
                failed += 1;
            }
        }
    }

    drop(quiet_guard);

    let elapsed = started.elapsed().as_secs_f64();
    let status = if failed == 0 { colour(&config, "ok", Colour::Green) } else { colour(&config, "FAILED", Colour::Red) };
    println!(
        "\ntest result: {status}. {passed} passed; {failed} failed; 0 ignored; 0 measured; 0 filtered out; finished in {elapsed:.2}s"
    );
    if failed > 0 {
        process::exit(1);
    }
}

#[derive(Debug)]
struct HarnessConfig {
    filters: Vec<String>,
    list: bool,
    help: bool,
    colour: ColourMode,
    verbose: bool,
}

impl HarnessConfig {
    fn from_args(args: &[OsString]) -> Self {
        let mut filters = Vec::new();
        let mut list = false;
        let mut help = false;
        let mut colour = ColourMode::Auto;
        let mut verbose = env_flag("FEYE_W3C_RDF_VERBOSE");
        let mut i = 0;
        while i < args.len() {
            let arg = args[i].to_string_lossy();
            match arg.as_ref() {
                "--list" => list = true,
                "-h" | "--help" => help = true,
                "--nocapture" | "--show-output" | "--format" | "pretty" | "terse" | "json" => {}
                "--exact" | "--skip" | "--test-threads" | "--report-time" | "--ensure-time" => {
                    i += 1;
                }
                "--color" | "--colour" => {
                    if let Some(value) = args.get(i + 1).map(|s| s.to_string_lossy()) {
                        colour = ColourMode::from_str(&value);
                        i += 1;
                    }
                }
                "--verbose" => verbose = true,
                other if other.starts_with("--color=") => {
                    colour = ColourMode::from_str(other.trim_start_matches("--color="));
                }
                other if other.starts_with('-') => {}
                other => filters.push(other.to_string()),
            }
            i += 1;
        }
        HarnessConfig { filters, list, help, colour, verbose }
    }

    fn matches(&self, label: &str) -> bool {
        self.filters.is_empty() || self.filters.iter().any(|filter| label.contains(filter))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColourMode { Auto, Always, Never }

impl ColourMode {
    fn from_str(value: &str) -> Self {
        match value {
            "always" => ColourMode::Always,
            "never" => ColourMode::Never,
            _ => ColourMode::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Colour { Green, Red }

fn colour(config: &HarnessConfig, text: &str, colour: Colour) -> String {
    if !use_colour(config) {
        return text.to_string();
    }
    let code = match colour {
        Colour::Green => "32",
        Colour::Red => "31",
    };
    format!("\x1b[{code}m{text}\x1b[0m")
}

fn use_colour(config: &HarnessConfig) -> bool {
    if env::var_os("NO_COLOR").is_some() {
        return false;
    }
    match config.colour {
        ColourMode::Always => true,
        ColourMode::Never => false,
        ColourMode::Auto => env::var("CARGO_TERM_COLOR").map(|value| value != "never").unwrap_or(true),
    }
}

enum TestOutcome { Ok, Failed }

fn print_result(config: &HarnessConfig, label: &str, outcome: TestOutcome, detail: &str) {
    match outcome {
        TestOutcome::Ok => println!("test {label} ... {} ({detail})", colour(config, "ok", Colour::Green)),
        TestOutcome::Failed => println!("test {label} ... {}\n    {detail}", colour(config, "FAILED", Colour::Red)),
    }
}

struct QuietGuard { previous: Option<OsString>, changed: bool }

impl QuietGuard {
    fn enable_unless_verbose(verbose: bool) -> Self {
        if verbose {
            return QuietGuard { previous: None, changed: false };
        }
        let previous = env::var_os("FEYE_W3C_RDF_QUIET");
        env::set_var("FEYE_W3C_RDF_QUIET", "1");
        QuietGuard { previous, changed: true }
    }
}

impl Drop for QuietGuard {
    fn drop(&mut self) {
        if !self.changed {
            return;
        }
        if let Some(previous) = self.previous.take() {
            env::set_var("FEYE_W3C_RDF_QUIET", previous);
        } else {
            env::remove_var("FEYE_W3C_RDF_QUIET");
        }
    }
}

fn print_help() {
    println!("Custom W3C RDF conformance harness for Feye");
    println!();
    println!("Usage:");
    println!("  cargo test --test w3c_rdf");
    println!("  cargo test --test w3c_rdf rdf11_turtle");
    println!();
    println!("The harness prints libtest-style lines with coloured ok/FAILED status words.");
    println!("It is local-only by default; use FEYE_W3C_RDF_REFRESH=1 to bootstrap or refresh tests/w3c_rdf/rdf-tests/.");
    println!();
    println!("Useful environment variables:");
    println!("  FEYE_W3C_RDF_REFRESH=1   bootstrap/refresh the local W3C RDF mirror");
    println!("  FEYE_W3C_RDF_VERBOSE=1   show runner progress in addition to harness lines");
    println!("  FEYE_W3C_RDF_FILTER=txt  filter W3C cases inside a selected manifest");
    println!("  FEYE_W3C_RDF_EARL=PATH   choose the EARL report output path");
}

fn env_flag(name: &str) -> bool {
    env::var(name).map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")).unwrap_or(false)
}
