use std::io::Write;
use std::process::{Command, Stdio};

/// Runs `pg_format` over a SQL string, feeding it on stdin and returning the
/// reformatted output.
pub fn format_sql(sql: &str) -> String {
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
