use std::ops::Range;

use syn::{
    Expr, Token,
    punctuated::Punctuated,
    spanned::Spanned,
    visit::{self, Visit},
};

use crate::{QUERY, QUERY_AS, format};

pub struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
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

pub fn get_edits(src: &str) -> Vec<Edit> {
    let file = syn::parse_file(src).unwrap();

    let mut visitor = QueryMacroVisitor { macros: Vec::new() };
    visitor.visit_file(&file);

    let mut edits: Vec<Edit> = Vec::new();
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

        let formatted = format::format_sql(&sql);

        let range = sql_expr.span().byte_range();
        let indent = line_indent(src, range.start);
        edits.push(Edit {
            range,
            replacement: to_raw_string_literal(formatted.trim_end(), indent),
        });
    }

    edits
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

// Folds an expression made of string literals joined by `+` into a single
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
