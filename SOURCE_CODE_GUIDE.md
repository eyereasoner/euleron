# Eyeron Source Code Guide

This guide explains Eyeron for a computer science student who knows basic Rust but may be new to RDF, Notation3, or rule engines. It focuses on the program's structure and algorithms. For installation and command examples, see [`README.md`](README.md).

## 1. What the program does

Eyeron is a **reasoner**: it starts with facts and rules, applies the rules, and produces facts that logically follow.

Here is a small Notation3 (N3) program:

```n3
@prefix : <http://example.org/> .

:Socrates a :Human .

{ ?person a :Human . } => { ?person a :Mortal . } .
```

The first statement is a fact. The second is a rule: if some `?person` is human, conclude that the same person is mortal. Eyeron binds `?person` to `:Socrates` and derives:

```n3
:Socrates a :Mortal .
```

At a high level, the implementation resembles both a compiler and a database query engine:

```text
source text
    │
    ▼
lexer.rs ── tokens ──▶ parser.rs ── Document AST ──▶ reasoner.rs
                                                          │
                                                          ▼
                                                ReasonerResult
                                                          │
                                      ┌───────────────────┴──────────────────┐
                                      ▼                                      ▼
                                 printing.rs                             proof.rs
                                      │                                      │
                                      ▼                                      ▼
                                  N3 / TriG                         proof in N3
```

The command-line program in `main.rs`, the library API in `lib.rs`, and the browser API in `wasm.rs` are different front ends around this same pipeline.

## 2. Essential domain concepts

### RDF triples

Most knowledge is represented as a triple:

```text
(subject, predicate, object)
```

For example, `:Socrates a :Human` says that Socrates has RDF type Human. The subject is `:Socrates`, the predicate is `a` (short for `rdf:type`), and the object is `:Human`.

### Terms

Each position in a triple contains a **term**. Eyeron supports:

- IRIs, which are globally unique names;
- variables such as `?person`;
- blank nodes, which identify unnamed resources;
- literals such as strings and numbers;
- lists;
- quoted formulas, which contain triples used as data.

### Rules and bindings

A rule has a premise (body) and a conclusion (head). A **binding** is a map from variable names to terms:

```text
?person → :Socrates
```

If all premises match under one binding, the reasoner substitutes those values into the conclusion.

### Closure and fixpoint

The **closure** is the set of explicit and derived facts known so far. New facts may enable more rules, so rules are applied repeatedly. The process stops at a **fixpoint**, when one complete pass produces no new facts.

## 3. Repository map

| Path | Responsibility |
| --- | --- |
| `src/ast.rs` | Core data structures: terms, triples, rules, and documents |
| `src/lexer.rs` | Converts input characters into tokens |
| `src/parser.rs` | Builds the AST from N3 and RDF syntax |
| `src/reasoner.rs` | Matching, unification, forward/backward reasoning, indexes, and built-ins |
| `src/printing.rs` | Serializes results as N3, TriG, debug text, or JSON |
| `src/proof.rs` | Builds and renders explanations for derived facts |
| `src/error.rs` | Error type and source-position reporting |
| `src/rdf_compat.rs` | Selects Turtle, N-Triples, N-Quads, or TriG parser profiles |
| `src/lib.rs` | Public Rust library API and module exports |
| `src/main.rs` | Native command-line interface |
| `src/wasm.rs` | WebAssembly/browser interface |
| `src/bin/w3c_rdf.rs` | Helper binary for RDF conformance work |
| `tools/build_playground.rs` | Builds the browser playground package |
| `examples/` | Example inputs, expected outputs, and proof outputs |
| `tests/` | Integration, regression, CLI, N3, and W3C RDF tests |

The best reading order is `ast.rs`, `lib.rs`, `lexer.rs`, selected parts of `parser.rs`, and then the main loop near `reasoner::reason`. Read individual built-in functions only when you need them.

## 4. The abstract syntax tree (`src/ast.rs`)

The AST is the shared language between parsing, reasoning, and printing.

### `Term`

`Term` is a Rust enum:

```rust
pub enum Term {
    Iri(String),
    Var(String),
    Blank(String),
    Literal(Literal),
    List(Vec<Term>),
    Formula(Vec<Triple>),
}
```

An enum is a good fit because a term is exactly one of these variants. Recursive variants (`List` and `Formula`) allow arbitrarily nested data. `Term::is_ground` recursively checks that a term contains no variables. Groundness matters because a ground fact can be stored as established knowledge, while a variable-containing triple is normally a pattern.

### `Triple`, `Rule`, and `Document`

`Triple` stores subject, predicate, and object terms. `Rule` stores vectors of premise and conclusion triples, plus flags distinguishing forward rules, backward rules, and queries.

`Document` is the parser's final product. It contains:

- prefixes and an optional base IRI;
- explicit facts;
- source locations for proof reporting;
- rules.

`Document::merge` makes multi-file input simple: the CLI parses each input separately and combines the resulting documents.

Many AST types derive `Eq`, `Ord`, and `Hash`. This is important, not cosmetic: the reasoner can place terms and triples in maps and sets for indexing, deterministic output, and duplicate detection.

## 5. Lexing (`src/lexer.rs`)

The lexer performs the first translation:

```text
":Socrates a :Human ."
             ↓
[PName, A, PName, Dot, Eof]
```

`TokenKind` lists the language's vocabulary, including names, variables, literals, punctuation, formula braces, list parentheses, and implication arrows. Every `Token` also stores its byte offset. That offset later becomes a useful line-and-column parse error.

`lex` constructs a private `Lexer` and calls `run`. The loop:

1. skips whitespace and comments;
2. examines the next character(s);
3. chooses a specialized reader for an IRI, string, number, name, and so on;
4. appends a token;
5. finishes with an explicit `Eof` token.

Keeping lexing separate simplifies the parser: the parser asks “is this an arrow token?” instead of repeatedly interpreting characters.

## 6. Parsing (`src/parser.rs` and `src/rdf_compat.rs`)

The public entry points include:

- `parse_n3` for normal N3;
- `parse_n3_with_source` when proof output needs source locations;
- `parse_rdf12` for RDF formats;
- `parse_rdf_message_log` for message logs.

Internally, `Parser` owns the token vector, a current position, the `Document` under construction, counters for generated blank nodes, and a `ParserProfile`. Profiles let one parser enforce different grammar restrictions for N3, Turtle, TriG, N-Triples, and N-Quads.

`parse_document` repeatedly examines the next token and dispatches to a more specific method. Prefix and base declarations update parser state. Ordinary statements become facts. Formula implications such as `{ ... } => { ... }` become `Rule` values.

Some compact RDF syntax expands into multiple triples. For example, semicolons reuse a subject and lists may create structural triples in RDF-compatible modes. This is why several parsing functions return both a term and a vector of generated triples.

RDF Message Logs are split at message boundaries. Each payload is parsed and represented using message-envelope vocabulary and quoted formulas, allowing rules to inspect a message as one atomic graph.

## 7. Reasoning (`src/reasoner.rs`)

This is the largest module because it contains the core algorithm and the implementations of built-in predicates.

### Result and options

`ReasonerOptions` supplies safety limits and enables tracing or proof capture. Limits prevent recursive or accidentally unbounded programs from running forever.

`ReasonerResult` separates several useful views:

- `explicit`: input facts;
- `derived`: newly inferred output facts;
- `closure`: explicit plus derived facts;
- `proofs`: derivation records;
- `status`, limits, errors, and statistics.

This distinction explains why normal output does not repeat every input fact.

### Fact indexing

A naive matcher would scan every fact for every premise. `FactIndex` instead stores fact positions in three maps:

```text
predicate                 → fact positions
(subject, predicate)      → fact positions
(predicate, object)       → fact positions
```

When some pattern positions are already ground, `candidates` selects a much smaller set. It falls back to a full scan where necessary, preserving correctness. This is the same basic idea as a database index.

### Matching and unification

`Bindings` is a `BTreeMap<String, Term>`. `match_triple` matches three term pairs. In `match_term`:

- an unbound pattern variable is assigned the corresponding value;
- an already bound variable must agree with its earlier value;
- lists match element by element;
- formulas are unified as unordered collections of triples;
- concrete terms must be semantically equal.

`unify_term` is more general because either side may contain a variable. It also performs an **occurs check** before creating a binding, avoiding cyclic substitutions such as `?x = (?x)`.

For multiple premises, `match_premise_remaining` is a recursive backtracking search. It does not blindly use source order. It prefers a runnable, selective premise with few candidates. This resembles join ordering in a relational database: applying a selective condition early avoids creating many intermediate bindings.

### The forward-chaining fixpoint

The main `reason` function works approximately as follows:

```text
put explicit facts in the closure and indexes
separate query rules from materialization rules

repeat:
    drive agenda-safe rules from newly added facts
    evaluate other forward rules with general matching
    instantiate conclusions and insert unseen facts
    if a conclusion creates a new rule, register it
until no unseen fact was added

evaluate query rules against the completed closure
return explicit facts, derived facts, closure, proofs, and statistics
```

A `HashSet<Triple>` named `seen` prevents duplicate facts and is also the fixpoint signal. `emit_conclusions` substitutes a successful binding into each rule conclusion. Blank nodes in a conclusion are created deterministically for that firing. `insert_materialized_triple` updates the closure, index, derived output, and optional proof record together.

### Agenda optimization

For suitable forward rules, `AgendaIndex` maps a newly added fact directly to rule premises that it might satisfy. A one-premise transitive chain can therefore process each new fact once instead of rescanning all rules and facts on every iteration.

Rules that need more context—certain multi-premise forms, built-ins, backward dependencies, or blank-node conclusions—use the general matcher. The optimized path is an implementation detail; both paths preserve the same logical behavior.

### Backward rules

Forward reasoning asks, “What can be concluded from all known facts?” Backward reasoning starts with a goal and asks, “Which rule could prove this goal, and what would prove that rule's premises?”

`solve_backward_goal` recursively searches backward rules. Rules are **standardized apart** before use: their variables are renamed so variables from separate rule applications cannot collide. A recursion stack detects cycles, while depth and solution limits bound the search.

### Built-ins

`eval_builtin` dispatches by predicate IRI to functions implementing operations such as arithmetic, string matching, list processing, time extraction, hashing, and logical formula inspection.

A built-in consumes the current binding and returns zero or more bindings:

- zero means the premise failed;
- one means it succeeded once, possibly binding a result variable;
- several means it generated several possible results.

This interface lets ordinary fact matching and built-in computation participate in the same backtracking search.

### Queries

`log:query` rules are deliberately held until the normal rules reach a fixpoint. They select or format results without feeding their output back into reasoning. A conclusion using `log:outputString` is treated specially by the printer and emitted as plain text.

## 8. Proofs (`src/proof.rs`)

When `ReasonerOptions::proof` is enabled, every derived fact can store:

- the fact that was derived;
- the rule that fired;
- the instantiated premises;
- the variable bindings.

`proof_to_n3` groups these `DerivedFact` records, recursively connects derived premises to their own derivations, and distinguishes explicit facts, built-in steps, and unproven steps. Source references recorded during parsing let proofs point back to input files and lines.

Proof collection is optional because retaining derivation history costs memory. Normal reasoning only needs the closure and derived facts.

## 9. Output (`src/printing.rs`)

The printer reverses part of the parsing process: AST values become text again. It handles correct syntax for every `Term` variant, escapes strings, chooses compact prefixes, formats formulas and lists, and can produce N3 or TriG.

Only prefixes actually used in the result are printed. This keeps output smaller and is why the printer first walks the output to collect used namespaces.

`result_to_string` and `rdf_result_to_string` first look for `log:outputString`. If present, literal values are concatenated as program output; otherwise triples are serialized normally.

## 10. Front ends

### Rust library (`src/lib.rs`)

The convenience function `eyeron::reason` parses one string, runs default reasoning, rejects incomplete results, and returns rendered derived facts. Lower-level callers can separately parse, configure `ReasonerOptions`, call `reason_document`, and inspect the structured result.

### Command line (`src/main.rs`)

The CLI performs these steps:

1. parse flags;
2. read local files, standard input, or HTTP(S) resources;
3. infer the RDF format from a file extension when appropriate;
4. parse and merge all documents;
5. either print the AST or run the reasoner;
6. print normal, RDF-compatible, or proof output.

The message-streaming path reads and reasons over one message at a time. Each message gets a fresh reasoning run, so facts are not retained across messages.

### WebAssembly (`src/wasm.rs`)

The Wasm module exposes browser-friendly functions through `wasm-bindgen`. It accepts a program and optional data, selects a parser format, invokes the same core reasoner, and converts errors and reports to JavaScript-friendly strings or JSON. The reasoning algorithm is not duplicated.

## 11. Error handling

`EyeronError` contains a message and optional byte offset. The lexer and parser attach offsets; `with_source_location` converts them into file, line, and column information for users.

Reasoning-time problems are represented separately in `ReasonerResult`. A run may have produced a partial closure while also reaching a search limit. `CompletionStatus` makes this explicit, and high-level APIs reject incomplete results rather than silently presenting them as complete.

## 12. A complete example trace

For the Socrates example, the important state changes are:

1. The lexer recognizes prefix syntax, three terms for the fact, formula delimiters, variables, and the implication arrow.
2. The parser expands `a` to the full `rdf:type` IRI. It stores one explicit `Triple` and one forward `Rule` in a `Document`.
3. `reason` inserts the explicit triple into `closure`, `seen`, and `FactIndex`.
4. The agenda finds that the new fact may satisfy the rule's only premise.
5. `match_triple` binds `?person` to `:Socrates`.
6. `emit_conclusions` resolves `?person` in the rule head and constructs the mortal triple.
7. The new triple is not in `seen`, so it is appended to `closure` and `derived`.
8. No further rule produces an unseen fact, so the fixpoint is reached.
9. `printing.rs` shortens the IRIs using the input prefixes and writes the derived triple.

This trace is a useful debugging template: inspect tokens, then the `Document`, bindings, closure changes, and finally rendering.

## 13. Tests and how to explore the code

Run all optimized tests:

```bash
cargo test --release
```

Useful focused commands include:

```bash
# See the parsed representation without reasoning
cargo run -- --ast examples/socrates.n3

# Run a small forward-rule example
cargo run -- examples/socrates.n3

# Include derivation explanations
cargo run -- --proof examples/socrates.n3

# Run one integration-test target
cargo test --test regressions
```

Unit tests live beside implementation code, while `tests/` contains black-box and conformance tests. The examples have expected results in `examples/output/` and expected explanations in `examples/proof/`, which makes them especially useful when learning or changing behavior.

## 14. Suggested study exercises

1. Add a fact to `examples/socrates.n3`, run with `--ast`, and identify its `Term` variants.
2. Set a breakpoint in `match_term` and watch `Bindings` change as a rule matches.
3. Compare a one-premise rule with a two-premise rule and observe the agenda and general matching paths.
4. Follow `math:sum` from `eval_builtin` to its implementation, then write a tiny N3 program that uses it.
5. Run with and without `--proof` and compare the `ReasonerResult` data retained.
6. Add a small parser or reasoner regression test before modifying behavior.

## 15. Design lessons

Several broader computer science ideas appear in this project:

- **Separation of concerns:** lexing, parsing, evaluation, proof construction, and serialization are separate modules.
- **Algebraic data types:** `Term` models several forms safely with a Rust enum.
- **Hashing and ordering:** sets provide fast duplicate detection; ordered maps provide stable behavior.
- **Database techniques:** indexes and selective join ordering reduce matching work.
- **Search algorithms:** premise matching uses backtracking, and backward rules use bounded depth-first search.
- **Fixed-point computation:** forward chaining monotonically grows a set until it stops changing.
- **Resource safety:** explicit limits turn nontermination into a structured incomplete result.
- **Reusable core:** the CLI, library, and browser interfaces share one parser and reasoning engine.

The central idea to keep in mind is simple: Eyeron converts syntax into structured triples and rules, searches for consistent variable bindings, materializes new ground triples, and repeats until knowledge stops growing.
