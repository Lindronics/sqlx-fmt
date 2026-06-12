use clap::Parser;
use sqlx_fmt::{Mode, run};

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

    /// Print the diff.
    #[arg(short, long)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();

    let mode = if args.check {
        Mode::Check
    } else {
        Mode::Format
    };
    run(&args.path, mode, args.verbose);
}
