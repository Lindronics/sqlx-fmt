use insta::assert_snapshot;
use sqlx_fmt::fmt_str;
use sqlx_fmt::format::PgFormat;

fn fmt(src: &str) -> String {
    fmt_str(&PgFormat::new(vec![]), src)
}

#[test]
fn query_single_line_literal() {
    assert_snapshot!(fmt(
        r#"fn f() { let _ = sqlx::query!("select id, name from users where id = $1", id); }"#
    ));
}

#[test]
fn query_as_with_type_argument() {
    assert_snapshot!(fmt(
        r#"fn f() { let _ = query_as!(User, "select * from users where active = true"); }"#
    ));
}

#[test]
fn concatenated_string_literals() {
    assert_snapshot!(fmt(
        r#"fn f() { let _ = query!("select id " + "from users " + "where id = $1", id); }"#
    ));
}

#[test]
fn multiline_raw_string() {
    assert_snapshot!(fmt(
        "fn f() {\n    let _ = query!(r#\"select id, name\n        from users\n        where active\"#);\n}\n"
    ));
}

#[test]
fn multiple_macros_in_one_file() {
    let src = r#"
async fn run(pool: &PgPool) -> Result<()> {
    let user = sqlx::query!("select id from users where id = $1", id)
        .fetch_one(pool)
        .await?;
    let all = query_as!(User, "select id, name from users order by name")
        .fetch_all(pool)
        .await?;
    Ok(())
}
"#;
    assert_snapshot!(fmt(src));
}

#[test]
fn source_without_macros_is_unchanged() {
    let src = "fn main() {\n    let x = 1;\n}\n";
    assert_snapshot!(fmt(src));
}
