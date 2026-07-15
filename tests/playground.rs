use std::fs;
use std::path::Path;

#[test]
fn playground_html_is_packaged_for_browser_wasm() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let playground = root.join("playground.html");
    let html = fs::read_to_string(&playground)
        .unwrap_or_else(|err| panic!("failed to read {}: {}", playground.display(), err));

    assert!(
        html.contains("The Eyeron N3 Playground"),
        "{}",
        playground.display()
    );
    assert!(
        html.contains("./pkg/eyeron.js"),
        "playground should load the wasm-pack web bundle"
    );
    assert!(
        html.contains("reasonWithData"),
        "playground should expose separate data + N3 program reasoning"
    );

    let examples_dir = root.join("examples");
    let mut expected = fs::read_dir(&examples_dir)
        .expect("read examples directory")
        .map(|entry| entry.expect("read examples entry").path())
        .filter(|path| {
            path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("n3")
        })
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    expected.sort();

    let list = html
        .split("const bundledExamples = [")
        .nth(1)
        .and_then(|tail| tail.split("];").next())
        .expect("playground bundledExamples array");
    let mut actual = list
        .lines()
        .filter_map(|line| {
            line.trim()
                .trim_end_matches(',')
                .strip_prefix('"')
                .and_then(|line| line.strip_suffix('"'))
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(
        expected, actual,
        "playground bundledExamples must list every top-level N3 example"
    );
}
