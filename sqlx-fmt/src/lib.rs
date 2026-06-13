use std::path::Path;

pub mod syntax;

pub const QUERY: &str = "query";
pub const QUERY_AS: &str = "query_as";

/// What to do with the formatted output, mirroring rustfmt's emit modes.
#[derive(Clone, Copy)]
pub enum Emit {
    /// Overwrite each input file in place (rustfmt default).
    Files,
    /// Print the formatted file to stdout.
    Stdout,
    /// Print a diff and never write; used by `--check`.
    Diff,
}

/// Output/behaviour switches shared across all files in one invocation.
pub struct Options {
    pub backup: bool,
    pub color: bool,
    pub files_with_diff: bool,
    pub verbose: bool,
    pub quiet: bool,
}

/// Formats a single file according to `emit`. Returns `true` if the file was
/// **not** already correctly formatted (i.e. formatting changed something).
pub fn run(path: &Path, emit: Emit, opts: &Options) -> bool {
    if opts.verbose {
        eprintln!("Formatting {}", path.display());
    }

    let src = std::fs::read_to_string(path).unwrap();
    let formatted_src = format_str(&src);
    let changed = src != formatted_src;

    match emit {
        Emit::Stdout => print!("{formatted_src}"),
        Emit::Diff => {
            if changed {
                if opts.files_with_diff {
                    println!("{}", path.display());
                } else if !opts.quiet {
                    print_diff(&src, &formatted_src, opts.color);
                }
            }
        }
        Emit::Files => {
            if changed {
                if opts.backup {
                    let mut backup = path.as_os_str().to_owned();
                    backup.push(".bk");
                    std::fs::copy(path, backup).unwrap();
                }
                write_atomic(path, &formatted_src).unwrap();
                if opts.files_with_diff && !opts.quiet {
                    println!("{}", path.display());
                }
            }
        }
    }

    changed
}

/// Applies every SQL edit to `src` and returns the reformatted source.
fn format_str(src: &str) -> String {
    let mut edits = syntax::get_edits(src);

    // Apply edits from the end of the file backwards so earlier byte offsets
    // stay valid as we mutate the string.
    edits.sort_by_key(|&Edit { start, .. }| std::cmp::Reverse(start));

    let mut formatted_src = src.to_string();
    for edit in edits {
        formatted_src.replace_range(edit.start..edit.end, &edit.replacement);
    }
    formatted_src
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

/// Prints a line-level unified diff between two versions of the file,
/// optionally colored.
fn print_diff(original: &str, formatted: &str, color: bool) {
    use similar::{ChangeTag, TextDiff};

    let diff = TextDiff::from_lines(original, formatted);
    for change in diff.iter_all_changes() {
        let (sign, ansi) = match change.tag() {
            ChangeTag::Delete => ("-", "\x1b[31m"), // red
            ChangeTag::Insert => ("+", "\x1b[32m"), // green
            ChangeTag::Equal => (" ", "\x1b[2m"),   // dim
        };
        if color {
            print!("{ansi}{sign}{change}\x1b[0m");
        } else {
            print!("{sign}{change}");
        }
    }
}

pub struct Edit {
    start: usize,
    end: usize,
    replacement: String,
}
