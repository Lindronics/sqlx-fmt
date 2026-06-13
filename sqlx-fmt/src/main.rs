use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use sqlx_fmt::{Emit, Options, fmt_file};

/// Format the SQL inside sqlx `query!` / `query_as!` macros.
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Files to format.
    #[arg(required = true)]
    files: Vec<PathBuf>,

    /// Run in 'check' mode. Exits with 0 if input is formatted correctly.
    /// Exits with 1 and prints a diff if formatting is required.
    #[arg(long)]
    check: bool,

    /// What data to emit and how.
    #[arg(long, default_value = "files")]
    emit: EmitArg,

    /// Backup any modified files.
    #[arg(long)]
    backup: bool,

    /// Use colored output (if supported).
    #[arg(long, default_value = "auto")]
    color: ColorArg,

    /// Prints the names of mismatched files that were formatted. Prints the
    /// names of files that would be formatted when used with `--check` mode.
    #[arg(short = 'l', long = "files-with-diff")]
    files_with_diff: bool,

    /// Print verbose output.
    #[arg(short, long)]
    verbose: bool,

    /// Print less output.
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum EmitArg {
    Files,
    Stdout,
}

#[derive(Clone, Copy, ValueEnum)]
enum ColorArg {
    Always,
    Never,
    Auto,
}

impl From<&Args> for Options {
    fn from(args: &Args) -> Self {
        // `--check` overrides `--emit`, mirroring rustfmt.
        let emit = if args.check {
            Emit::Diff
        } else {
            match args.emit {
                EmitArg::Files => Emit::Files,
                EmitArg::Stdout => Emit::Stdout,
            }
        };

        let color = match args.color {
            ColorArg::Always => true,
            ColorArg::Never => false,
            ColorArg::Auto => std::io::IsTerminal::is_terminal(&std::io::stdout()),
        };

        Options {
            backup: args.backup,
            color,
            files_with_diff: args.files_with_diff,
            verbose: args.verbose,
            quiet: args.quiet,
            emit,
        }
    }
}

fn main() {
    let args = Args::parse();

    let opts = Options::from(&args);

    let mut needs_formatting = false;
    for path in &args.files {
        needs_formatting |= fmt_file(path, &opts);
    }

    // In check mode, a required reformat is a failure (exit 1).
    if matches!(opts.emit, Emit::Diff) && needs_formatting {
        std::process::exit(1);
    }
}
