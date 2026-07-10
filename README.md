# Eyeron

Eyeron is like a calculator for facts. You give it things you know and rules to follow; it figures out what else must be true, and when you ask why, it shows the steps. That makes your data easier to check, share, and build on.

Eyeron is a Rust command-line and library implementation of a core Notation3/N3 reasoner. It reads one or more N3 files, applies forward rules and goal-directed backward rules, evaluates a practical subset of common N3 built-ins, and writes derived output as N3 or direct text produced by `log:outputString`. The native integration target for structured RDF exchange is RDF Messages.

Eyeron is the Rust reasoner in the Eyereasoner family. **Eyeling** remains the sibling project; this package intentionally uses the `eyeron` crate name and `eyeron` executable name to keep the projects distinct.

The crate provides both:

- a command-line program named `eyeron`; and
- a library API for embedding the reasoner in Rust applications.

## Features

Eyeron currently supports the core constructs used by the bundled examples:

- prefixes and bases with common default N3/RDF prefixes;
- triples, variables, blank nodes, blank-node property lists, literals, RDF lists, and quoted formulas;
- forward rules: `{ body } => { head }`;
- goal-directed backward rules: `{ head } <= { body }`;
- generated rules, including rules emitted from rule heads;
- quoted formula unquoting in rule conclusions, such as `{ :x :formula ?F } => ?F .`;
- `log:query` rule form;
- `=` as `owl:sameAs`;
- RDF list matching through first-class list terms and virtual `rdf:first` / `rdf:rest`;
- deterministic generated blank nodes for repeated rule firings;
- indexed fact lookup and an agenda path for safe single-premise rule chains.

Implemented built-in families include a practical subset of:

- `log:`: equality, inequality, query, `includes`/`notIncludes`, collect/conjunction/conclusion helpers, URI conversion, and scoped formula operations used by the examples;
- `math:`: sum, difference, numeric comparisons, and numeric equality/inequality;
- `list:`: first, rest, firstRest, last, length, member, memberAt, in, notMember, remove, append, reverse, sort, iterate, and map;
- `string:`: comparison, concatenation, containment, starts/ends-with, regex match/not-match, replace, scrape, and simple formatting;
- `time:`: year, month, day, hour, minute, second, and time-zone extraction.

## Build

```bash
cargo build --release
```

The optimized binary will be available at:

```bash
target/release/eyeron
```

## Command-line use

Run an example:

```bash
cargo run -- examples/witch.n3
```

After a release build, run the binary directly:

```bash
target/release/eyeron examples/witch.n3
```

Run multiple input files as one merged document:

```bash
cargo run -- file1.n3 file2.n3
```

Read from standard input:

```bash
echo '@prefix : <http://example.org/> . :Socrates a :Man . { ?x a :Man . } => { ?x a :Mortal . } .' | cargo run -- -
```

Show help/version:

```bash
cargo run -- --help
cargo run -- --version
```

RDF-compatible input is selected by file extension. Eyeron recognizes `.ttl`, `.nt`, `.nq`, and `.trig`; standard input can be parsed as Turtle with `--rdf`:

```bash
cargo run -- input.ttl
cargo run -- input.nt
cargo run -- input.nq
cargo run -- input.trig
cargo run -- --rdf --base-iri https://example.org/base - < input.ttl
```

The CLI accepts a small set of legacy Eyereasoner flags such as `--ast`, `--proof`, `--rdf`, `--stream`, `--builtin`, `--store`, and `--store-path`. `-p`/`--proof` emits N3 proof explanations, and `-s`/`--stream` keeps the current finite-output behavior. Flags that are not otherwise implemented by Eyeron are accepted as no-ops or warnings so existing command lines fail softly during migration.

## Example

Input:

```n3
@prefix : <http://example.org/> .

:Socrates a :Man .

{
  ?x a :Man .
} => {
  ?x a :Mortal .
} .
```

Output:

```n3
@prefix : <http://example.org/> .

:Socrates a :Mortal .
```

Proof output can be enabled with `-p`/`--proof`:

```bash
cargo run -- --proof examples/socrates.n3
```

In proof mode Eyeron prints an N3 proof document using the `pe:` vocabulary. Each derived triple is connected to a quoted proof graph with `pe:why`, rule applications are marked with `pe:by`, instantiated premises with `pe:uses`, and rule substitutions with `pe:binding`. Eyeling-style proof goldens are bundled under `examples/proof/`.

## Library use

Add the crate as a dependency from this repository or workspace, then call `eyeron::reason`:

```rust
fn main() -> eyeron::Result<()> {
    let output = eyeron::reason(r#"
      @prefix : <http://example.org/> .

      :Socrates a :Man .

      {
        ?x a :Man .
      } => {
        ?x a :Mortal .
      } .
    "#)?;

    assert!(output.contains(":Socrates a :Mortal"));
    Ok(())
}
```

For lower-level use, parse first and call the reasoner directly:

```rust
use eyeron::{parse_n3, reason_document, result_to_string, ReasonerOptions};

fn run(input: &str) -> eyeron::Result<String> {
    let doc = parse_n3(input, None)?;
    let result = reason_document(&doc, &ReasonerOptions::default());
    Ok(result_to_string(&doc.prefixes, &result.derived))
}
```


## RDF Messages

RDF Messages support is a primary integration target for Eyeron. Files that use the draft replay syntax

```text
VERSION "1.2-messages"
...
MESSAGE
...
MESSAGE
...
```

are parsed as RDF Message Logs. Eyeron materializes an internal replay view using the `eymsg:` vocabulary:

- one `eymsg:RDFMessageStream` resource;
- one `eymsg:MessageEnvelope` per message boundary;
- `eymsg:firstEnvelope`, `eymsg:nextEnvelope`, and `eymsg:orderedEnvelopes` links;
- `eymsg:payloadKind eymsg:nonEmpty` or `eymsg:payloadKind eymsg:empty`;
- one payload graph resource per non-empty message, connected to a quoted formula with `log:nameOf`.

Rules can inspect payloads atomically with `log:includes` without merging message bodies into the ordinary fact graph. Blank-node labels in the message-log input are scoped per message before payload formulas are exposed to reasoning, and UTF-8 string literals are preserved during message replay.

Run the basic RDF Messages example with its message-log sidecar:

```bash
cargo run -- -r examples/rdf-messages.n3 examples/input/rdf-messages.trig
```

Other bundled RDF Message Log examples:

```bash
cargo run -- -r examples/rdf-message-flow.n3 examples/input/rdf-message-flow.trig
cargo run -- -r examples/rdf-message-microgrid.n3 examples/input/rdf-message-microgrid.trig
cargo run -- -r examples/rdf-message-window-repair.n3 examples/input/rdf-message-window-repair.trig
cargo run -- -r examples/rdf-message-ldes-incremental.n3 examples/input/rdf-message-ldes-incremental.trig
cargo run -- -r examples/rdf-message-cold-chain-recall.n3 examples/input/rdf-message-cold-chain-recall.trig
```

The current implementation is replay-oriented: it reads a finite RDF Message Log and reasons over the exposed replay view. Continuous streaming input/output remains future work.

## Testing

Run the full validation suite with:

```bash
cargo test
```

The test suite includes:

- parser and built-in unit tests;
- focused regression tests for backward rules, generated rules, quoted formulas, list handling, and blank nodes;
- a packaged example/golden-output sweep over the examples in `examples/` and `examples/output/`;
- proof-output goldens under `examples/proof/` for `cargo test --release`;
- the bundled Notation3 conformance suite from `tests/notation3tests`, checked by `tests/notation3_conformance.rs`.

During the golden and conformance sweeps, progress and elapsed time are printed so long-running cases are visible:

```text
checking examples/hanoi.n3
ok examples/hanoi.n3 (0.012s)
```

The example comparison checks stable output lines rather than exact byte-for-byte output. This avoids false failures caused by derived triple ordering or generated blank-node labels. The Notation3 conformance test mirrors the upstream score model: success tests must derive the expected result, fail tests must not derive it, and crash tests are accepted only when they do not expose normal test results.

The bundled examples include rule, list, string, log, time, algebraic, policy/alignment, RDF Message Log, and deep-taxonomy workloads. The five `deep-taxonomy-*` examples are included in the normal `cargo test` sweep and exercise the agenda-based single-premise rule path.

### W3C RDF 1.x manifests and EARL report

Eyeron keeps RDF manifest work inside Rust. The full W3C RDF sweep lives in `tests/w3c_rdf/` with a thin Cargo integration-test entry point at `tests/w3c_rdf.rs`. It runs the 12 RDF 1.1 / RDF 1.2 manifest roots, follows `mf:include`, executes syntax, eval, and RDF/RDFS entailment cases through Eyeron's shared lexer/parser profiles, and writes an EARL report.

The runner uses a local W3C RDF mirror under `tests/w3c_rdf/rdf-tests/`. Network access is disabled by default so `cargo test` cannot silently spend minutes downloading files; if the mirror is missing, the test fails fast with a bootstrap instruction. The W3C RDF sweep is now part of the normal test suite:

```bash
cargo test
```

The W3C RDF integration test uses a small custom harness so ordinary `cargo test` output remains explicit and the status words are colored when Cargo colors are enabled. It prints one line per manifest plus one aggregate EARL-report line, e.g. `test w3c_rdf_01_rdf11_n_triples_70 ... ok (70 tests)`, `test w3c_rdf_07_rdf11_turtle_313 ... ok (313 tests)`, and `test w3c_rdf_13_all_manifests_1170_earl_report ... ok (1170 tests + EARL report)`.

To run only the W3C RDF sweep:

```bash
cargo test --test w3c_rdf
```

To force colored status words even when Cargo is not attached to a terminal:

```bash
cargo test --test w3c_rdf -- --color always
```

The aggregate test writes the default output report:

```text
reports/w3c-rdf-earl.ttl
```

Useful environment variables:

```bash
EYERON_W3C_RDF_EARL=reports/w3c-rdf-earl.ttl cargo test --test w3c_rdf
EYERON_W3C_RDF_FILTER=turtle12 cargo test --test w3c_rdf
EYERON_W3C_RDF_QUIET=1 cargo test --test w3c_rdf
EYERON_W3C_RDF_VERBOSE=1 cargo test --test w3c_rdf
EYERON_W3C_RDF_REFRESH=1 cargo test --test w3c_rdf
EYERON_W3C_RDF_ONLINE=1 cargo test --test w3c_rdf
EYERON_W3C_RDF_CACHE_DIR=tests/w3c_rdf/rdf-tests cargo test --test w3c_rdf
```


During `EYERON_W3C_RDF_REFRESH=1`, the 12 per-manifest harness lines are reported as delegated and the aggregate pass performs the single online mirror refresh. You can still select a subset by passing a substring filter such as `cargo test --test w3c_rdf rdf11_turtle`.

The milestone target is **1170/1170 tests passed across 12 manifests**. The per-manifest harness checks assert their expected counts (`70`, `29`, `87`, `27`, `48`, `77`, `313`, `29`, `74`, `356`, `25`, `35`) and the aggregate check asserts `1170`. Keep `tests/w3c_rdf/rdf-tests/` under version control for fast, reproducible local runs. The runner does not introduce a second RDF parser: Turtle, N-Triples, N-Quads, TriG, RDF 1.2 triple terms, and RDF 1.2 annotations are handled as syntax profiles over the same lexer/parser infrastructure used by N3.

## Examples

Some useful examples to run manually:

```bash
cargo run -- examples/socrates.n3
cargo run -- examples/witch.n3
cargo run -- examples/family-cousins.n3
cargo run -- examples/backward.n3
cargo run -- examples/derived-rule.n3
cargo run -- examples/derived-backward-rule.n3
cargo run -- examples/rdf-list.n3
cargo run -- examples/list-builtins-tests.n3
cargo run -- examples/string-builtins-tests.n3
cargo run -- examples/hanoi.n3
cargo run -- examples/gray-code-counter.n3
cargo run -- examples/deep-taxonomy-100000.n3
cargo run -- -r examples/rdf-messages.n3 examples/input/rdf-messages.trig
```

## Performance notes

The reasoner maintains indexes for grounded fact lookup and includes a fast agenda path for safe single-premise forward rules. This keeps long taxonomy chains close to linear in practice. Recursive and blank-node-heavy state-machine examples can still require more selective scheduling.

## Non-goals

Eyeron targets native Rust execution, command-line use, embedding from Rust, and RDF Messages integration.

## Current limitations

This first release focuses on the core reasoning path and the bundled example suite. The following areas are intentionally limited or incomplete:

- continuous RDF Messages streaming input/output beyond finite log replay;
- external URL dereferencing;
- persistent fact stores;
- proof trace comments and full explanation output;
- custom external built-in modules;
- complete coverage of every N3 built-in namespace.


## Project layout

```text
src/
  ast.rs        Core syntax/data structures
  lexer.rs      Tokenizer
  parser.rs     Shared N3/RDF 1.2 parser profiles
  reasoner.rs   Rule engine and built-ins
  printing.rs   N3/debug output and RDF 1.2 JSON adapter output used by tests
  main.rs       CLI entry point
examples/       Example N3 inputs
examples/output Golden outputs used by cargo test
examples/proof  Eyeling-style proof-output goldens
tests/          Integration and golden-sweep tests
  notation3tests/ Bundled Notation3 conformance data
  w3c_rdf/        Rust W3C RDF 1.x manifest sweep
```

## License

MIT. See `LICENSE.md`.
