use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};

use crate::{Emit, Options, fmt_file, format::PgFormat};

/// Format the SQL inside sqlx `query!` / `query_as!` macros.
#[derive(Parser)]
#[command(version, about)]
pub struct Args {
    /// Files to format. When run as `cargo sqlx-fmt` with no files given, every
    /// Rust file in the surrounding cargo workspace is formatted.
    pub files: Vec<PathBuf>,

    /// Run in 'check' mode. Exits with 0 if input is formatted correctly.
    /// Exits with 1 and prints a diff if formatting is required.
    #[arg(long)]
    pub check: bool,

    /// What data to emit and how.
    #[arg(long, default_value = "files")]
    pub emit: EmitArg,

    /// Backup any modified files.
    #[arg(long)]
    pub backup: bool,

    /// Use colored output (if supported).
    #[arg(long, default_value = "auto")]
    pub color: ColorArg,

    /// Prints the names of mismatched files that were formatted. Prints the
    /// names of files that would be formatted when used with `--check` mode.
    #[arg(short = 'l', long = "files-with-diff")]
    pub files_with_diff: bool,

    /// Print verbose output.
    #[arg(short, long)]
    pub verbose: bool,

    /// Print less output.
    #[arg(short, long)]
    pub quiet: bool,

    /// Arguments to pass to inner SQL formatter.
    #[arg(long)]
    pub extra_args: Vec<String>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum EmitArg {
    Files,
    Stdout,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ColorArg {
    Always,
    Never,
    Auto,
}

/// Formats every requested file, returning a process exit code. In check mode,
/// a file that needs reformatting yields a failure code.
pub fn run(args: Args) -> ExitCode {
    if args.files.is_empty() {
        eprintln!("error: no files to format");
        return ExitCode::FAILURE;
    }

    let opts = Options {
        backup: args.backup,
        color: match args.color {
            ColorArg::Always => true,
            ColorArg::Never => false,
            ColorArg::Auto => std::io::IsTerminal::is_terminal(&std::io::stdout()),
        },
        files_with_diff: args.files_with_diff,
        verbose: args.verbose,
        quiet: args.quiet,
        // `--check` overrides `--emit`, mirroring rustfmt.
        emit: if args.check {
            Emit::Diff
        } else {
            match args.emit {
                EmitArg::Files => Emit::Files,
                EmitArg::Stdout => Emit::Stdout,
            }
        },
    };

    // TODO(nl): Support multiple formatters
    let formatter = PgFormat::new(args.extra_args);

    let mut needs_formatting = false;
    for path in &args.files {
        needs_formatting |= fmt_file(&formatter, path, &opts);
    }

    if matches!(opts.emit, Emit::Diff) && needs_formatting {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
