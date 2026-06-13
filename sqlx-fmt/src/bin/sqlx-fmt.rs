use std::process::ExitCode;

use clap::Parser;
use sqlx_fmt::cli::{Args, run};

fn main() -> ExitCode {
    run(Args::parse())
}
