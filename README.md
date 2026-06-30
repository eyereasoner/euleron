# Eyeling

Eyeling is a small Rust implementation of a core Notation3/N3 reasoner. It reads one or more N3 files, applies forward and goal-directed backward rules, evaluates a practical subset of common N3 built-ins, and writes the derived output as N3 or direct text produced by `log:outputString`.

The crate provides both:

- a command-line program named `eyeling`; and
- a library API for embedding the reasoner in Rust applications.

## Features

Eyeling currently supports the core constructs used by the bundled examples:

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
- an indexed fact store and agenda path for single-premise rules, including large taxonomy-style rule chains.

Implemented built-in families include a practical subset of:

- `log:`: equality, inequality, query, collect/conjunction/conclusion helpers, URI conversion, scoped formula operations used by the examples;
- `math:`: sum, difference, numeric comparisons, numeric equality/inequality;
- `list:`: first, rest, firstRest, last, length, member, memberAt, in, notMember, remove, append, reverse, sort, iterate, map;
- `string:`: comparison, concatenation, containment, starts/ends-with, regex match/not-match, replace, scrape, and simple formatting;
- `time:`: year, month, day, hour, minute, second, time zone extraction.

## Build

```bash
cargo build --release
```

The optimized binary will be available at:

```bash
target/release/eyeling
```

## Command-line use

Run an example:

```bash
cargo run -- examples/witch.n3
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

Accepted compatibility flags include `--ast`, `--proof`, `--rdf`, `--stream`, `--super-restricted`, `--deterministic-skolem`, `--builtin`, `--store`, and `--store-path`. Some compatibility flags are currently accepted as no-ops or warnings so existing command lines are easier to migrate.

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

Add the crate as a dependency from this repository or workspace, then call `eyeling::reason`:

```rust
fn main() -> eyeling::Result<()> {
    let output = eyeling::reason(r#"
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
use eyeling::{parse_n3, reason_document, result_to_string, ReasonerOptions};

fn run(input: &str) -> eyeling::Result<String> {
    let doc = parse_n3(input, None)?;
    let result = reason_document(&doc, &ReasonerOptions::default());
    Ok(result_to_string(&doc.prefixes, &result.derived))
}
```

## Testing

Run the full validation suite with:

```bash
cargo test
```

The test suite includes:

- parser and built-in unit tests;
- focused regression tests for backward rules, generated rules, quoted formulas, list handling, and blank nodes;
- a packaged example/golden-output sweep over the examples in `examples/` and `examples/output/`.

During the golden sweep, progress and elapsed time are printed for each example:

```text
checking examples/hanoi.n3
ok examples/hanoi.n3 (0.012s)
```

The comparison checks stable output lines rather than exact byte-for-byte output. This avoids false failures caused by derived triple ordering or generated blank-node labels.

The bundled examples include rule, list, string, log, time, algebraic, policy/alignment, and deep-taxonomy workloads. The five `deep-taxonomy-*` examples are included in the normal `cargo test` sweep and exercise the agenda-based single-premise rule path.

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
```

## Performance notes

The reasoner maintains indexes for grounded fact lookup and includes a fast agenda path for safe single-premise forward rules. This keeps long taxonomy chains close to linear in practice. Recursive and blank-node-heavy state-machine examples can still require more selective scheduling.

## Current limitations

This first release focuses on the core reasoning path and the bundled example suite. The following areas are intentionally limited or incomplete:

- full RDF 1.2 / TriG parsing modes;
- RDF-JS-compatible data model APIs;
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
