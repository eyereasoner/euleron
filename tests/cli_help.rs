use std::process::Command;

#[test]
fn no_arguments_is_the_same_as_short_help() {
    let no_args = Command::new(env!("CARGO_BIN_EXE_eyeron"))
        .output()
        .expect("run eyeron without arguments");
    let short_help = Command::new(env!("CARGO_BIN_EXE_eyeron"))
        .arg("-h")
        .output()
        .expect("run eyeron -h");

    assert_eq!(no_args.status, short_help.status);
    assert_eq!(no_args.stdout, short_help.stdout);
    assert_eq!(no_args.stderr, short_help.stderr);
}

#[test]
fn reasoning_limit_flags_are_not_accepted() {
    for flag in [
        "--max-iterations",
        "--max-match-steps",
        "--max-backward-depth",
        "--max-backward-solutions",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_eyeron"))
            .args([flag, "1"])
            .output()
            .expect("run eyeron with removed flag");

        assert!(!output.status.success(), "{flag} should be rejected");
        assert_eq!(output.stdout, b"");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(stderr.as_ref(), format!("eyeron: unknown option {flag}\n"));
    }
}
