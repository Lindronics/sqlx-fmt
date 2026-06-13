use std::io::Write;
use std::process::{Command, Stdio};

/// Formatter for SQL.
pub trait SqlFormatter {
    fn format_sql(&self, sql: &str) -> String;
}

/// Postgres formatter.
/// Requires [https://github.com/darold/pgFormatter] to be installed.
pub struct PgFormat {
    args: Vec<String>,
}

impl PgFormat {
    pub fn new(args: Vec<String>) -> Self {
        Self { args }
    }
}

impl SqlFormatter for PgFormat {
    fn format_sql(&self, sql: &str) -> String {
        let mut child = Command::new("pg_format")
            .arg("-") // read from stdin
            .args(&self.args)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_format_passes_through_extra_args() {
        // `--no-space-function` is a flag pg_format accepts; passing it must not
        // break invocation, and output should still be produced.
        let out = PgFormat::new(vec!["--comma-end".to_string()]).format_sql("select 1, 2");
        assert!(out.contains('1') && out.contains('2'), "got: {out:?}");
    }
}
