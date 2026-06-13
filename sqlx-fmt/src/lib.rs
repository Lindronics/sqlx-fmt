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
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.range.start));

    let mut formatted_src = src.to_string();
    for syntax::Edit { range, replacement } in edits {
        formatted_src.replace_range(range, &replacement);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(emit: Emit) -> Options {
        Options {
            backup: false,
            color: false,
            files_with_diff: false,
            verbose: false,
            quiet: true,
            emit,
        }
    }

    #[test]
    fn fmt_str_leaves_already_formatted_source_untouched() {
        // A file with no query macros is never changed.
        let src = "fn main() {\n    let x = 1;\n}\n";
        assert_eq!(fmt_str(src), src);
    }

    #[test]
    fn fmt_str_is_idempotent() {
        let src = r#"fn f() { let _ = sqlx::query!("select id from t where id=1"); }"#;
        let once = fmt_str(src);
        assert_ne!(once, src, "expected formatting to change the source");
        assert_eq!(fmt_str(&once), once, "second pass should be a no-op");
    }

    #[test]
    fn fmt_str_rewrites_only_the_sql_keeping_surrounding_code() {
        let src = r#"fn f() { let _ = query!("select 1", bind_arg); }"#;
        let out = fmt_str(src);
        // Surrounding tokens (binding arg, trailing punctuation) are preserved.
        assert!(out.starts_with("fn f() { let _ = query!("), "got: {out}");
        assert!(out.ends_with(", bind_arg); }"), "got: {out}");
        assert!(out.contains("SELECT"), "got: {out}");
    }

    #[test]
    fn fmt_file_files_mode_writes_changes_and_reports_changed() {
        let path = std::env::temp_dir().join(format!("sqlx-fmt-lib-{}.rs", std::process::id()));
        std::fs::write(&path, r#"fn f() { let _ = query!("select 1"); }"#).unwrap();

        let changed = fmt_file(&path, &opts(Emit::Files));

        assert!(changed);
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("SELECT"), "file not rewritten: {written}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn fmt_file_diff_mode_does_not_modify_the_file() {
        let path =
            std::env::temp_dir().join(format!("sqlx-fmt-lib-diff-{}.rs", std::process::id()));
        let original = r#"fn f() { let _ = query!("select 1"); }"#;
        std::fs::write(&path, original).unwrap();

        let changed = fmt_file(&path, &opts(Emit::Diff));

        assert!(
            changed,
            "check mode should report the file needs formatting"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            original,
            "diff/check mode must never write"
        );
        let _ = std::fs::remove_file(&path);
    }
}
