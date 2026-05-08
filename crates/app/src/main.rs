//! `f9-talk` binary entry point: clap CLI + secrets loader + single-instance
//! lock + wires every other crate together.
//!
//! Status: stub for M1 — currently just verifies the toolchain links.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "f9-talk", version, about = "Hold-to-talk dictation for Linux")]
struct Cli {
    /// STT backend selection.
    #[arg(long, value_enum, default_value_t = Backend::Cloud)]
    backend: Backend,

    /// Hotkey for the local Whisper backend (held while speaking).
    #[arg(long, default_value = "f9")]
    local_hotkey: String,

    /// Hotkey for the cloud backend (held while speaking).
    #[arg(long, default_value = "f8")]
    cloud_hotkey: String,

    /// Translate transcripts to this ISO language code before typing.
    #[arg(long)]
    target: Option<String>,

    /// Domain-specific terms to bias STT toward (proper nouns, jargon). Repeatable.
    #[arg(long = "keyword")]
    keywords: Vec<String>,

    /// Indicator animation style. v1.0 only ships `wave`; others fall back to wave.
    #[arg(long, default_value = "wave")]
    style: String,

    /// Verbose (debug-level) logging.
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum Backend {
    Cloud,
    Local,
    Both,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let _ = cli;
    eprintln!("f9-talk {}: M1 scaffold OK.", env!("CARGO_PKG_VERSION"));
    Ok(())
}
