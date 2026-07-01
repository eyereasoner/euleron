# Reports

The aggregate W3C RDF custom-harness check `w3c_rdf_13_all_manifests_1170_earl_report` writes the EARL report here by default. The test resources live under `tests/w3c_rdf/rdf-tests/`; the runner is local-only by default so repeat runs stay fast:

```bash
cargo test
```

To run only the W3C RDF sweep:

```bash
cargo test --test w3c_rdf
```

```text
reports/w3c-rdf-earl.ttl
```

The target milestone for this report is 1170/1170 tests passed across the 12 default RDF 1.1 / RDF 1.2 manifest roots.
