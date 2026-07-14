use std::process::Command;

#[test]
fn streams_turtle_style_rdf_message_directives() {
    let output = Command::new(env!("CARGO_BIN_EXE_euleron"))
        .args([
            "-r",
            "--stream-messages",
            "examples/alma-rdf-messages.n3",
            "tests/input/alma-rdf-messages-small.nt",
        ])
        .output()
        .expect("run euleron");

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout,
        "<http://lib.ugent.be/record/9936831849109161> <http://example.org/ns#title> \"Young's Market Company, LLC SWOT Analysis\" .\n"
    );
}
