# W3C RDF manifest sweep

This directory contains Feye's Rust-only W3C RDF 1.x manifest runner.

It is part of the normal `cargo test` suite. By default this test is **local-only**: it reads `tests/w3c_rdf/rdf-tests/` and fails fast if the mirror is missing. This keeps repeat runs comparable to `tests/notation3tests/` instead of silently spending minutes on HTTP fetches.

```bash
cargo test
```

The integration test uses a custom harness and prints 13 libtest-style lines: 12 per-manifest checks named with their expected counts, plus `w3c_rdf_13_all_manifests_1170_earl_report`. The status words are colored when Cargo colors are enabled. This makes plain `cargo test` output more informative than a single opaque `w3c_rdf_manifests ... ok` line.

To run only the W3C RDF sweep:

```bash
cargo test --test w3c_rdf
```

To force colors:

```bash
cargo test --test w3c_rdf -- --color always
```

To bootstrap or refresh the local mirror, run one explicit online pass:

```bash
FEYE_W3C_RDF_REFRESH=1 cargo test --test w3c_rdf
```

After that, commit `tests/w3c_rdf/rdf-tests/`. Normal runs are then local.
During `FEYE_W3C_RDF_REFRESH=1`, the 12 per-manifest harness lines are reported as delegated and the aggregate pass performs the single online mirror refresh. You can run a subset with a substring filter, for example `cargo test --test w3c_rdf rdf11_turtle`.


The runner loads the 12 default RDF 1.1 / RDF 1.2 manifest roots from the local mirror, follows `mf:include`, executes syntax/eval/RDF-RDFS entailment cases through Feye's shared parser profiles, and the aggregate test writes:

```text
reports/w3c-rdf-earl.ttl
```

Useful environment variables:

```bash
FEYE_W3C_RDF_EARL=reports/w3c-rdf-earl.ttl
FEYE_W3C_RDF_FILTER=turtle12
FEYE_W3C_RDF_QUIET=1
FEYE_W3C_RDF_VERBOSE=1
FEYE_W3C_RDF_REFRESH=1       # explicit online bootstrap/refresh
FEYE_W3C_RDF_ONLINE=1        # allow network for cache misses without forcing refresh
FEYE_W3C_RDF_OFFLINE=1       # redundant default; documents intent
FEYE_W3C_RDF_CACHE_DIR=tests/w3c_rdf/rdf-tests
```

Current milestone target: 1170/1170 tests passed across 12 manifests. Per-manifest expected counts are 70, 29, 87, 27, 48, 77, 313, 29, 74, 356, 25, and 35. Keep the populated `tests/w3c_rdf/rdf-tests/` directory under version control for fast, reproducible local runs.
