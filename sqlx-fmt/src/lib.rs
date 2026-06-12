use std::path::Path;

pub mod syntax;

pub const QUERY: &str = "query";
pub const QUERY_AS: &str = "query_as";

pub enum Mode {
    Check,
    Format,
}

pub fn run(path: &Path, mode: Mode, verbose: bool) {
    let src = std::fs::read_to_string(path).unwrap();

    let mut edits = syntax::get_edits(&src);

    // Apply edits from the end of the file backwards so earlier byte offsets
    // stay valid as we mutate the string.
    edits.sort_by_key(|&Edit { start, .. }| std::cmp::Reverse(start));

    let mut formatted_src = src.clone();
    for edit in edits {
        formatted_src.replace_range(edit.start..edit.end, &edit.replacement);
    }

    if src != formatted_src {
        if verbose {
            print_diff(&src, &formatted_src);
        }
        match mode {
            Mode::Check => {
                std::process::exit(1);
            }
            Mode::Format => {
                write_atomic(path, &formatted_src).unwrap();
            }
        }
    }
}

/// Writes `contents` to `path` atomically: write to a temp file in the same
/// directory, then rename over the target. The rename is atomic on a single
/// filesystem, so a crash mid-write can't truncate or corrupt the original.
fn write_atomic(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();

    // Keep the temp file in the same directory so the rename stays on one
    // filesystem (cross-device renames fail and aren't atomic).
    let tmp = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));

    if let Err(e) = std::fs::write(&tmp, contents) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, path)
}

/// Prints a colored, line-level unified diff between two versions of the file.
fn print_diff(original: &str, formatted: &str) {
    use similar::{ChangeTag, TextDiff};

    let diff = TextDiff::from_lines(original, formatted);
    for change in diff.iter_all_changes() {
        let (sign, color) = match change.tag() {
            ChangeTag::Delete => ("-", "\x1b[31m"), // red
            ChangeTag::Insert => ("+", "\x1b[32m"), // green
            ChangeTag::Equal => (" ", "\x1b[2m"),   // dim
        };
        print!("{color}{sign}{change}\x1b[0m");
    }
}

pub struct Edit {
    start: usize,
    end: usize,
    replacement: String,
}
