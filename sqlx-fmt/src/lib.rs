use std::path::Path;

mod format;
mod io;
mod syntax;

pub const QUERY: &str = "query";
pub const QUERY_AS: &str = "query_as";

/// File formatting options.
pub struct Options {
    /// Back up files before formatting in place.
    pub backup: bool,
    /// Use ANSI colours for output.
    pub color: bool,
    /// Print names of files requiring formatting.
    pub files_with_diff: bool,
    /// Print more information.
    pub verbose: bool,
    /// Print less information.
    pub quiet: bool,
    /// Emit mode.
    pub emit: Emit,
}

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

/// Formats a single file according to `emit`. Returns `true` if the file was
/// **not** already correctly formatted (i.e. formatting changed something).
pub fn fmt_file(path: &Path, opts: &Options) -> bool {
    if opts.verbose {
        eprintln!("Formatting {}", path.display());
    }

    let src = std::fs::read_to_string(path).unwrap();
    let formatted_src = fmt_str(&src);
    let is_changed = src != formatted_src;

    match (opts.emit, is_changed) {
        (Emit::Stdout, _) => print!("{formatted_src}"),
        (Emit::Diff, true) => {
            if opts.files_with_diff {
                println!("{}", path.display());
            } else if !opts.quiet {
                print_diff(&src, &formatted_src, opts.color);
            }
        }
        (Emit::Files, true) => {
            if opts.backup {
                io::backup_file(path).unwrap();
            }
            io::write_atomic(path, &formatted_src).unwrap();
            if opts.files_with_diff && !opts.quiet {
                println!("{}", path.display());
            }
        }
        _ => {}
    }

    is_changed
}

/// Format `str`, returning the formatted source.
pub fn fmt_str(src: &str) -> String {
    let mut edits = syntax::get_edits(src);

    // Apply edits from the end of the file backwards so earlier byte offsets
    // stay valid as we mutate the string.
    edits.sort_by_key(|&syntax::Edit { start, .. }| std::cmp::Reverse(start));

    let mut formatted_src = src.to_string();
    for syntax::Edit { start, end, new } in edits {
        formatted_src.replace_range(start..end, &new);
    }
    formatted_src
}

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
