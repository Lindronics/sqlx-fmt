use std::ops::Range;

use syn::{
    Expr, Token,
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{self, Visit},
};

use crate::{QUERY, QUERY_AS};

/// An edit that can be applied to a given string.
pub struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
}

/// A SQL string literal located in the source.
pub struct EditTarget {
    pub range: Range<usize>,
    pub sql: String,
}

/// Collects every `query!` / `query_as!` macro invocation in a parsed file.
struct QueryMacroVisitor<'ast> {
    pub macros: Vec<&'ast syn::Macro>,
}

impl<'ast> Visit<'ast> for QueryMacroVisitor<'ast> {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if let Some(segment) = mac.path.segments.last()
            && matches!(segment.ident.to_string().as_str(), QUERY | QUERY_AS)
        {
            self.macros.push(mac);
        }

        // Keep walking — macros can be nested inside other macro inputs too.
        visit::visit_macro(self, mac);
    }
}

/// Finds the SQL string argument of every `query!` / `query_as!` invocation,
/// returning its source byte range and statically-concatenated contents.
///
/// This is the pure, `pg_format`-free half of [`get_edits`]: it does all the
/// macro detection and argument extraction so it can be unit tested on its own.
pub fn sql_targets(src: &str) -> Vec<EditTarget> {
    let file = syn::parse_file(src).unwrap();

    let mut visitor = QueryMacroVisitor { macros: Vec::new() };
    visitor.visit_file(&file);

    let mut targets = Vec::new();
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

        targets.push(EditTarget {
            range: sql_expr.span().byte_range(),
            sql,
        });
    }

    targets
}

/// Wraps SQL in a Rust raw-string literal, picking enough `#` hashes that the
/// content can't prematurely terminate the literal.
///
/// Single-line SQL stays inline (`r#"SELECT 1"#`). Multiline SQL always opens
/// with a newline and indents every line — plus the closing delimiter — by
/// `indent`, so it nests cleanly inside the surrounding code.
pub fn to_raw_string_literal(sql: &str, indent: &str) -> String {
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
pub fn line_indent(src: &str, offset: usize) -> &str {
    let line_start = src[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent_len = src[line_start..]
        .bytes()
        .take_while(|&b| b == b' ' || b == b'\t')
        .count();
    &src[line_start..line_start + indent_len]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_expr(s: &str) -> Expr {
        syn::parse_str(s).unwrap()
    }

    #[test]
    fn extract_sql_single_literal() {
        assert_eq!(
            extract_sql(&parse_expr(r#""SELECT 1""#)).as_deref(),
            Some("SELECT 1")
        );
    }

    #[test]
    fn extract_sql_concatenation() {
        let e = parse_expr(r#""SELECT " + "id " + "FROM t""#);
        assert_eq!(extract_sql(&e).as_deref(), Some("SELECT id FROM t"));
    }

    #[test]
    fn extract_sql_raw_string_preserves_newlines() {
        let e = parse_expr("r#\"SELECT\n  1\"#");
        assert_eq!(extract_sql(&e).as_deref(), Some("SELECT\n  1"));
    }

    #[test]
    fn extract_sql_unwraps_parentheses() {
        let e = parse_expr(r#"("a" + "b")"#);
        assert_eq!(extract_sql(&e).as_deref(), Some("ab"));
    }

    #[test]
    fn extract_sql_rejects_non_literal() {
        assert_eq!(extract_sql(&parse_expr("format!(\"{x}\")")), None);
        assert_eq!(extract_sql(&parse_expr("some_var")), None);
    }

    #[test]
    fn extract_sql_rejects_non_string_in_concatenation() {
        // A non-string leaf anywhere poisons the whole fold.
        assert_eq!(extract_sql(&parse_expr(r#""a" + 1"#)), None);
        assert_eq!(extract_sql(&parse_expr("1 + 2")), None);
    }

    #[test]
    fn extract_sql_rejects_non_add_operator() {
        // Concatenation is only `+`; other binary ops aren't string joins.
        assert_eq!(extract_sql(&parse_expr(r#""a" == "b""#)), None);
    }

    #[test]
    fn raw_literal_single_line_stays_inline() {
        assert_eq!(to_raw_string_literal("SELECT 1", "    "), "r#\"SELECT 1\"#");
    }

    #[test]
    fn raw_literal_multiline_opens_with_newline_and_indents() {
        let got = to_raw_string_literal("SELECT\n    1", "  ");
        assert_eq!(got, "r#\"\n  SELECT\n      1\n  \"#");
    }

    #[test]
    fn raw_literal_escalates_hashes_to_avoid_premature_close() {
        // Content containing `"#` forces a longer two-hash fence so the literal
        // can't terminate early.
        let got = to_raw_string_literal("a \"# b", "");
        assert!(got.starts_with("r##\""), "got: {got}");
        assert!(got.ends_with("\"##"), "got: {got}");
        // And it must round-trip back to a single literal containing the text.
        let reparsed: Expr = syn::parse_str(&got).unwrap();
        assert_eq!(extract_sql(&reparsed).as_deref(), Some("a \"# b"));
    }

    #[test]
    fn line_indent_at_start_of_file() {
        assert_eq!(line_indent("no indent", 0), "");
    }

    #[test]
    fn line_indent_spaces_and_tabs() {
        let src = "fn f() {\n\t    let x;\n}";
        let offset = src.find("let").unwrap();
        assert_eq!(line_indent(src, offset), "\t    ");
    }

    #[test]
    fn line_indent_stops_at_first_non_whitespace() {
        let src = "    code here";
        // Offset points into the middle of the line; indent is still the head.
        assert_eq!(line_indent(src, 8), "    ");
    }

    fn targets_sql(src: &str) -> Vec<String> {
        sql_targets(src).into_iter().map(|t| t.sql).collect()
    }

    #[test]
    fn detects_query_and_query_as_bare_and_qualified() {
        let src = r#"
            fn f() {
                let _ = query!("Q1");
                let _ = sqlx::query!("Q2");
                let _ = query_as!(User, "Q3");
                let _ = sqlx::query_as!(User, "Q4");
            }
        "#;
        assert_eq!(targets_sql(src), ["Q1", "Q2", "Q3", "Q4"]);
    }

    #[test]
    fn ignores_unrelated_and_lookalike_macros() {
        let src = r#"
            fn f() {
                println!("not sql");
                let _ = query_scalar!("also not handled");
                vec![1, 2, 3];
            }
        "#;
        assert!(targets_sql(src).is_empty());
    }

    #[test]
    fn finds_macros_nested_in_closures_and_blocks() {
        let src = r#"
            fn f() {
                let c = || { let _ = query!("NESTED"); };
                async { sqlx::query!("ASYNC") };
            }
        "#;
        assert_eq!(targets_sql(src), ["NESTED", "ASYNC"]);
    }

    #[test]
    fn skips_query_as_without_sql_argument() {
        // `query_as!` with only the type and no SQL: nothing to format.
        let src = r#"fn f() { let _ = query_as!(User); }"#;
        assert!(targets_sql(src).is_empty());
    }

    #[test]
    fn skips_non_literal_sql() {
        let src = r#"fn f() { let _ = query!(make_query()); }"#;
        assert!(targets_sql(src).is_empty());
    }

    #[test]
    fn target_range_covers_the_sql_argument() {
        let src = r#"fn f() { let _ = query!("PICK ME", bind); }"#;
        let targets = sql_targets(src);
        assert_eq!(targets.len(), 1);
        // The reported range should slice out exactly the SQL literal token.
        assert_eq!(&src[targets[0].range.clone()], r#""PICK ME""#);
    }

    #[test]
    fn concatenation_range_spans_all_pieces() {
        let src = r#"fn f() { let _ = query!("a" + "b" + "c"); }"#;
        let targets = sql_targets(src);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].sql, "abc");
        assert_eq!(&src[targets[0].range.clone()], r#""a" + "b" + "c""#);
    }
}
