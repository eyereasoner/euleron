# Eyeron

Eyeron is a Rust command-line and library implementation of a core Notation3/N3 reasoner. It reads one or more N3 files, applies forward rules and goal-directed backward rules, evaluates a practical subset of common N3 built-ins, and writes derived output as N3 or direct text produced by `log:outputString`. The native integration target for structured RDF exchange is RDF Messages.

Eyeron is the Rust reasoner. **Eyeling** is the JavaScript reasoner in the same Eyereasoner family; this package intentionally uses the `eyeron` crate name and `eyeron` executable name to keep the two projects distinct.

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

The CLI accepts a small set of legacy Eyereasoner flags such as `--ast`, `--proof`, `--rdf`, `--stream`, `--super-restricted`, `--deterministic-skolem`, `--builtin`, `--store`, and `--store-path`. Flags that are not implemented by Eyeron are accepted as no-ops or warnings so existing command lines fail softly during migration.

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
- the bundled Notation3 conformance suite from `tests/notation3tests`, checked by `tests/notation3_conformance.rs`.

During the golden and conformance sweeps, progress and elapsed time are printed so long-running cases are visible:

```text
checking examples/hanoi.n3
ok examples/hanoi.n3 (0.012s)
```

The example comparison checks stable output lines rather than exact byte-for-byte output. This avoids false failures caused by derived triple ordering or generated blank-node labels. The Notation3 conformance test mirrors the upstream score model: success tests must derive the expected result, fail tests must not derive it, and crash tests are accepted only when they do not expose normal test results.

The bundled examples include rule, list, string, log, time, algebraic, policy/alignment, RDF Message Log, and deep-taxonomy workloads. The five `deep-taxonomy-*` examples are included in the normal `cargo test` sweep and exercise the agenda-based single-premise rule path.

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

Eyeron does not target browser compatibility or RDF-JS compatibility. Those belong naturally to JavaScript environments. Eyeron targets native Rust execution, command-line use, embedding from Rust, and RDF Messages integration.

## Current limitations

This first release focuses on the core reasoning path and the bundled example suite. The following areas are intentionally limited or incomplete:

- full RDF 1.2 / TriG parsing modes;
- continuous RDF Messages streaming input/output beyond finite log replay;
- external URL dereferencing;
- persistent fact stores;
- proof trace comments and full explanation output;
- custom JavaScript built-in modules;
- complete coverage of every N3 built-in namespace.

`examples/dining-philosophers.n3` is packaged as an example, but it is skipped in the default golden sweep because it needs a more selective scheduler for blank-node-heavy state-machine joins.

## Project layout

```text
src/
  ast.rs        Core syntax/data structures
  lexer.rs      Tokenizer
  parser.rs     N3 parser
  reasoner.rs   Rule engine and built-ins
  printing.rs   N3/debug output
  main.rs       CLI entry point
examples/       Example N3 inputs
examples/output Golden outputs used by cargo test
tests/          Integration and golden-sweep tests
```

## License

MIT. See `LICENSE.md`.
