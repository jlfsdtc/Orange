//! orange-grep: Command-line grep tool reusing Orange's core search engine.

use std::path::PathBuf;
use std::io::{self, Write};

use clap::Parser;
use orange_core::LogData;
use orange_regex::{RegexEngine, RegexFlags};

#[derive(Parser)]
#[command(name = "orange-grep", about = "Fast log file grep tool")]
struct Args {
    /// Pattern to search for
    pattern: String,
    /// File to search in
    file: PathBuf,
    /// Use boolean expression mode (not yet implemented)
    #[arg(short, long)]
    boolean: bool,
    /// Case insensitive search
    #[arg(short, long)]
    ignore_case: bool,
    /// Print line numbers
    #[arg(short = 'n', long)]
    line_number: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.boolean {
        anyhow::bail!("--boolean mode is not implemented yet");
    }

    let log = LogData::open(&args.file)?;
    let engine = RegexEngine::compile(
        &args.pattern,
        RegexFlags { case_insensitive: args.ignore_case, dot_matches_newline: false },
    )
    .map_err(|e| anyhow::anyhow!("invalid regex: {e}"))?;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let total = log.line_count();
    for i in 0..total {
        let Some(line) = log.get_line(i) else { continue };
        if engine.scan_first(&line)?.is_some() {
            let res = (|| -> io::Result<()> {
                if args.line_number {
                    write!(out, "{}:", i + 1)?;
                }
                out.write_all(&line)?;
                out.write_all(b"\n")
            })();
            if let Err(e) = res {
                if e.kind() == io::ErrorKind::BrokenPipe {
                    return Ok(());
                }
                return Err(e.into());
            }
        }
    }
    Ok(())
}
