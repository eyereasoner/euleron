use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    let root = env::current_dir()?;
    println!("Building browser playground artifacts with wasm-pack...");
    let status = Command::new("wasm-pack")
        .arg("build")
        .arg("--target")
        .arg("web")
        .arg("--out-dir")
        .arg("pkg")
        .stdin(Stdio::null())
        .status();

    match status {
        Ok(status) if status.success() => {}
        Ok(status) => {
            return Err(io::Error::other(format!(
                "wasm-pack exited with status {status}"
            )));
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "wasm-pack was not found; install it with `cargo install wasm-pack`",
            ));
        }
        Err(err) => return Err(err),
    }

    for relative in ["pkg/.gitignore", "pkg/README.md", "pkg/LICENSE.md"] {
        remove_if_exists(root.join(relative))?;
    }

    println!("Cleaned generated package metadata not needed by GitHub Pages.");
    println!("Commit at least: playground.html, pkg/eyeron.js, pkg/eyeron_bg.wasm");
    Ok(())
}

fn remove_if_exists(path: impl AsRef<Path>) -> io::Result<()> {
    match fs::remove_file(path.as_ref()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}
