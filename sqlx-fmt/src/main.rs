use clap::Parser;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Expr, Token};

const QUERY: &str = "query";
const QUERY_AS: &str = "query_as";

/// Format the SQL inside sqlx `query!` / `query_as!` macros.
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Path to the Rust source file to process.
    path: std::path::PathBuf,

    /// Report whether the SQL is correctly formatted without writing changes;
    /// exits non-zero if any reformatting would be needed.
    #[arg(long)]
    check: bool,

    /// Print the diff even in --check mode.
    #[arg(long)]
    verbose: bool,
}

/// Collects every `query!` / `query_as!` macro invocation in a parsed file.
///
/// We visit *all* `syn::Macro` nodes rather than just top-level items, because
/// these macros are used in expression position and can appear arbitrarily deep
/// inside function bodies, closures, match arms, etc.
struct QueryMacroVisitor<'ast> {
    macros: Vec<&'ast syn::Macro>,
}

impl<'ast> Visit<'ast> for QueryMacroVisitor<'ast> {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        // Match on the final path segment so we catch both the bare `query!`
        // and the fully-qualified `sqlx::query!` forms.
        if let Some(segment) = mac.path.segments.last()
            && matches!(segment.ident.to_string().as_str(), QUERY | QUERY_AS)
        {
            self.macros.push(mac);
        }

        // Keep walking — macros can be nested inside other macro inputs too.
        visit::visit_macro(self, mac);
    }
}

fn main() {
    let args = Args::parse();

    let src = std::fs::read_to_string(&args.path).unwrap();
    let file = syn::parse_file(&src).unwrap();

    let mut visitor = QueryMacroVisitor { macros: Vec::new() };
    visitor.visit_file(&file);

    // Each edit replaces the source span of one SQL argument with a freshly
    // formatted raw-string literal. We collect them all, then splice them into
    // the source to build the rewritten file.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();

    for mac in &visitor.macros {
        let name = mac.path.segments.last().unwrap().ident.to_string();

        // Parse the raw macro tokens into a comma-separated argument list.
        let macro_args = mac
            .parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated)
            .unwrap();

        // The SQL string lives in a different position depending on the macro:
        //   query!(sql, binds...)              -> arg 0
        //   query_as!(Type, sql, binds...)     -> arg 1
        let sql_index = match name.as_str() {
            QUERY => 0,
            QUERY_AS => 1,
            _ => continue,
        };

        let Some(sql_expr) = macro_args.get(sql_index) else {
            continue;
        };
        let Some(sql) = extract_sql(sql_expr) else {
            continue;
        };

        let formatted = format_sql(&sql);
        let start = line_col_to_offset(&src, sql_expr.span().start());
        let end = line_col_to_offset(&src, sql_expr.span().end());
        let indent = line_indent(&src, start);
        edits.push((
            start,
            end,
            to_raw_string_literal(formatted.trim_end(), indent),
        ));
    }

    // Apply edits from the end of the file backwards so earlier byte offsets
    // stay valid as we mutate the string.
    edits.sort_by_key(|&(start, ..)| std::cmp::Reverse(start));
    let mut formatted_src = src.clone();
    for (start, end, replacement) in edits {
        formatted_src.replace_range(start..end, &replacement);
    }

    let changed = src != formatted_src;

    if args.check {
        // Don't touch the file; just report via exit code (diff only if asked).
        if args.verbose {
            print_diff(&src, &formatted_src);
        }
        if changed {
            std::process::exit(1);
        }
    } else {
        // Default: show what changed and write the formatted file back.
        print_diff(&src, &formatted_src);
        if changed {
            write_atomic(&args.path, &formatted_src).unwrap();
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

/// Runs `pg_format` over a SQL string, feeding it on stdin and returning the
/// reformatted output.
fn format_sql(sql: &str) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("pg_format")
        .arg("-") // read from stdin
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to spawn pg_format — is pgFormatter installed?");

    child
        .stdin
        .take()
        .unwrap()
        .write_all(sql.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Wraps SQL in a Rust raw-string literal, picking enough `#` hashes that the
/// content can't prematurely terminate the literal.
///
/// Single-line SQL stays inline (`r#"SELECT 1"#`). Multiline SQL always opens
/// with a newline and indents every line — plus the closing delimiter — by
/// `indent`, so it nests cleanly inside the surrounding code.
fn to_raw_string_literal(sql: &str, indent: &str) -> String {
    let mut hashes = String::from("#");
    while sql.contains(&format!("\"{hashes}")) {
        hashes.push('#');
    }

    if !sql.contains('\n') {
        return format!("r{hashes}\"{sql}\"{hashes}");
    }

    let body = sql
        .lines()
        .map(|line| format!("{indent}{line}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("r{hashes}\"\n{body}\n{indent}\"{hashes}")
}

/// Returns the leading whitespace of the line containing `offset`, used as the
/// base indentation for a reformatted SQL block.
fn line_indent(src: &str, offset: usize) -> &str {
    let line_start = src[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent_len = src[line_start..]
        .bytes()
        .take_while(|&b| b == b' ' || b == b'\t')
        .count();
    &src[line_start..line_start + indent_len]
}

/// Converts a proc-macro2 line/column (1-based line, 0-based char column) into
/// a byte offset into `src`.
fn line_col_to_offset(src: &str, lc: proc_macro2::LineColumn) -> usize {
    let mut offset = 0;
    for (i, line) in src.split_inclusive('\n').enumerate() {
        if i + 1 == lc.line {
            // Walk char boundaries; `lc.column` may point one past the last
            // char (end of a token), so fall back to the line's byte length.
            return offset
                + line
                    .char_indices()
                    .map(|(b, _)| b)
                    .nth(lc.column)
                    .unwrap_or(line.trim_end_matches('\n').len());
        }
        offset += line.len();
    }
    offset
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

/// Folds an expression made of string literals joined by `+` into a single
/// string, e.g. `"SELECT " + "* FROM users"` -> `SELECT * FROM users`.
///
/// Returns `None` if any leaf isn't a string literal (e.g. a `format!` or a
/// runtime value), since those can't be statically concatenated.
fn extract_sql(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) => Some(s.value()),
        Expr::Binary(syn::ExprBinary {
            left,
            op: syn::BinOp::Add(_),
            right,
            ..
        }) => Some(extract_sql(left)? + &extract_sql(right)?),
        // Unwrap parentheses / invisible groups so `("a" + "b")` still works.
        Expr::Paren(p) => extract_sql(&p.expr),
        Expr::Group(g) => extract_sql(&g.expr),
        _ => None,
    }
}
