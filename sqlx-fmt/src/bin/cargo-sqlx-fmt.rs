use std::process::ExitCode;

use clap::Parser;
use sqlx_fmt::cli::{Args, run};

/// When invoked as `cargo sqlx-fmt …`, cargo runs this binary with the
/// subcommand name (`sqlx-fmt`) as the first argument. This wrapper consumes
/// that token so the remaining arguments parse as the normal CLI.
#[derive(Parser)]
#[command(bin_name = "cargo")]
enum Cargo {
    SqlxFmt(Args),
}

fn main() -> ExitCode {
    let Cargo::SqlxFmt(args) = Cargo::parse();
    run(args)
}
