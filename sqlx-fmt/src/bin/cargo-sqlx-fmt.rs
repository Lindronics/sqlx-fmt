use std::{
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use clap::Parser;
use sqlx_fmt::cli::{Args, run};

#[derive(Parser)]
#[command(bin_name = "cargo")]
enum Cargo {
    SqlxFmt(Args),
}

fn main() -> ExitCode {
    let Cargo::SqlxFmt(mut args) = Cargo::parse();

    if args.files.is_empty() {
        args.files = workspace_rust_files();
    }

    // With no explicit files, format the whole workspace.
    run(args)
}

/// Collects every `.rs` file in the current cargo workspace, locating the
/// workspace root via `cargo locate-project` and skipping `target/` and hidden
/// directories.
fn workspace_rust_files() -> Vec<PathBuf> {
    let output = Command::new("cargo")
        .args(["locate-project", "--workspace", "--message-format", "plain"])
        .output()
        .expect("failed to run `cargo locate-project`");

    if !output.status.success() {
        eprintln!("error: `cargo locate-project` failed — not inside a cargo workspace?");
        std::process::exit(1);
    }

    let manifest = String::from_utf8_lossy(&output.stdout);
    let root = Path::new(manifest.trim())
        .parent()
        .expect("workspace manifest has no parent directory");

    let mut files = Vec::new();
    collect_rust_files(root, &mut files);
    files.sort();
    files
}

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // Skip build artifacts and hidden dirs like `.git`.
            if name == "target" || name.starts_with('.') {
                continue;
            }
            collect_rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}
