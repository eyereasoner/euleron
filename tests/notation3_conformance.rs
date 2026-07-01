use eyeron::{parse_n3, reason_document, result_to_string, ReasonerOptions, Term};
use std::fs;
use std::path::{Path, PathBuf};

const EX: &str = "http://example.org/";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

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

#[derive(Default)]
struct Score {
    count: usize,
    ok: usize,
    incomplete: usize,
    nonconform: usize,
    crashed: usize,
}

impl Score {
    fn add(&mut self, other: Score) {
        self.count += other.count;
        self.ok += other.ok;
        self.incomplete += other.incomplete;
        self.nonconform += other.nonconform;
        self.crashed += other.crashed;
    }
}

#[test]
fn notation3tests_conformance_suite() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("notation3tests")
        .join("tests");
    assert!(root.exists(), "notation3tests directory is missing: {}", root.display());

    let mut files = Vec::new();
    collect_n3_files(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "notation3tests suite is empty");

    progress_line(&format!("checking {} notation3tests files", files.len()));
    let started = std::time::Instant::now();
    let mut score = Score::default();
    let mut failures = Vec::new();

    for (idx, path) in files.iter().enumerate() {
        if idx % 100 == 0 || idx + 1 == files.len() {
            progress_line(&format!("notation3tests [{}/{}] {}", idx + 1, files.len(), path.strip_prefix(&root).unwrap_or(path).display()));
        }
        let result = run_one(path);
        if result.ok != result.count {
            failures.push(format!(
                "{} => ok:{} incomplete:{} nonconform:{} crashed:{} count:{}",
                path.strip_prefix(&root).unwrap_or(path).display(),
                result.ok,
                result.incomplete,
                result.nonconform,
                result.crashed,
                result.count
            ));
        }
        score.add(result);
    }

    let percent = if score.count == 0 { 0.0 } else { 100.0 * score.ok as f64 / score.count as f64 };
    progress_line(&format!(
        "notation3tests score: {:.1}% ({}/{}) in {:.3}s",
        percent,
        score.ok,
        score.count,
        started.elapsed().as_secs_f64()
    ));

    if !failures.is_empty() {
        panic!(
            "notation3tests did not reach 100%: ok={} count={} incomplete={} nonconform={} crashed={}\n{}",
            score.ok,
            score.count,
            score.incomplete,
            score.nonconform,
            score.crashed,
            failures.iter().take(30).cloned().collect::<Vec<_>>().join("\n")
        );
    }
}

fn collect_n3_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|err| panic!("failed to read {}: {}", dir.display(), err)) {
        let entry = entry.unwrap_or_else(|err| panic!("failed to read entry in {}: {}", dir.display(), err));
        let path = entry.path();
        if path.is_dir() {
            collect_n3_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("n3") {
            out.push(path);
        }
    }
}

fn run_one(path: &Path) -> Score {
    let is_crash = path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("crash"));

    let input = match fs::read_to_string(path) {
        Ok(input) => input,
        Err(_) => return Score { count: 1, crashed: 1, ..Score::default() },
    };

    let out = match parse_n3(&input, Some(EX)) {
        Ok(doc) => {
            let result = reason_document(&doc, &ReasonerOptions::default());
            result_to_string(&doc.prefixes, &result.derived)
        }
        Err(_) if is_crash => return Score { count: 1, ok: 1, ..Score::default() },
        Err(_) => return Score { count: 1, crashed: 1, ..Score::default() },
    };

    if out.trim().is_empty() {
        return if is_crash {
            Score { count: 1, ok: 1, ..Score::default() }
        } else {
            Score { count: 1, incomplete: 1, ..Score::default() }
        };
    }

    let output_doc = match parse_n3(&out, Some(EX)) {
        Ok(doc) => doc,
        Err(_) if is_crash => return Score { count: 1, ok: 1, ..Score::default() },
        Err(_) => return Score { count: 1, crashed: 1, ..Score::default() },
    };

    let tests = find_tests(&output_doc.facts);

    if is_crash {
        return if tests.is_empty() {
            Score { count: 1, ok: 1, ..Score::default() }
        } else {
            Score { count: 1, nonconform: 1, ..Score::default() }
        };
    }

    if tests.is_empty() {
        return Score { count: 1, incomplete: 1, ..Score::default() };
    }

    let mut score = Score::default();
    let has_test_true = has_test_boolean(&output_doc.facts, "true");
    let has_test_false = has_test_boolean(&output_doc.facts, "false");

    for test in tests {
        score.count += 1;
        let name = local_name(&test);
        if name.starts_with("fail") {
            if has_result_node(&output_doc.facts, &test) || has_test_true || has_test_false {
                score.nonconform += 1;
            } else {
                score.ok += 1;
            }
        } else if name.starts_with("success") {
            if has_result_node(&output_doc.facts, &test) || has_test_true {
                score.ok += 1;
            } else if has_test_false {
                score.nonconform += 1;
            } else {
                score.incomplete += 1;
            }
        } else {
            score.incomplete += 1;
        }
    }
    score
}

fn iri(value: &str) -> Term { Term::Iri(value.to_string()) }

fn find_tests(facts: &[eyeron::Triple]) -> Vec<Term> {
    let s = iri(&format!("{}test", EX));
    let p = iri(&format!("{}contains", EX));
    let mut tests: Vec<_> = facts.iter()
        .filter(|triple| triple.s == s && triple.p == p)
        .map(|triple| triple.o.clone())
        .collect();
    tests.sort();
    tests.dedup();
    tests
}

fn has_result_node(facts: &[eyeron::Triple], object: &Term) -> bool {
    let s = iri(&format!("{}result", EX));
    let p = iri(&format!("{}has", EX));
    facts.iter().any(|triple| triple.s == s && triple.p == p && &triple.o == object)
}

fn has_test_boolean(facts: &[eyeron::Triple], value: &str) -> bool {
    let s = iri(&format!("{}test", EX));
    let p = iri(&format!("{}is", EX));
    facts.iter().any(|triple| {
        if triple.s != s || triple.p != p { return false; }
        matches!(&triple.o, Term::Literal(lit) if lit.value == value && lit.datatype.as_deref() == Some(XSD_BOOLEAN))
    })
}

fn local_name(term: &Term) -> String {
    match term {
        Term::Iri(iri) => iri.rsplit(['/', '#']).next().unwrap_or(iri).to_string(),
        Term::Blank(id) => id.clone(),
        Term::Literal(lit) => lit.value.clone(),
        other => format!("{:?}", other),
    }
}
