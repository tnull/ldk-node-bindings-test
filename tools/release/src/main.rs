mod candidates;
mod model;
mod prepare;
mod process;
mod publish;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
#[derive(Parser)]
#[command(
    about = "Rebuild public binding candidates and publish verified bytes locally",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    Prepare {
        #[arg(long)]
        repo: String,
    },
    Inventory {
        #[arg(long)]
        dir: PathBuf,
        #[arg(long)]
        recipe: String,
        #[arg(long)]
        group: String,
        #[arg(long)]
        candidate: Option<PathBuf>,
        #[arg(long)]
        output: PathBuf,
    },
    Candidate {
        #[arg(long)]
        dir: PathBuf,
        #[arg(long)]
        recipe: String,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        attempt: String,
        #[arg(long)]
        output: PathBuf,
    },
    Verify {
        #[arg(long)]
        candidate: String,
        #[arg(long)]
        group: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Authenticate a public candidate and rebuild the groups supported by this host.
    VerifyCandidate {
        /// Public candidate release tag in lightningdevkit/ldk-node-bindings.
        id: String,
        /// Expected SHA-256 of the exact candidate.json bytes.
        sha256: String,
        /// Rebuild just this supported group instead of all host groups.
        #[arg(long)]
        group: Option<String>,
        /// Empty directory for downloads, pinned recipe, receipts, and diagnostics.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Collect {
        #[arg(long)]
        candidate: String,
        #[arg(long)]
        receipts: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Publish {
        #[arg(long)]
        verified: PathBuf,
        #[arg(long)]
        candidate: String,
        #[arg(long, value_enum)]
        destination: publish::Destination,
        #[arg(long)]
        confirm: String,
        #[command(flatten)]
        swift: publish::SwiftOptions,
    },
}
fn main() {
    if let Err(error) = execute() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
fn execute() -> Result<()> {
    let cli = Cli::parse();
    let root = process::root()?;
    match cli.command {
        Commands::Prepare { repo } => prepare::prepare(&root, &repo),
        Commands::Inventory {
            dir,
            recipe,
            group,
            candidate,
            output,
        } => candidates::inventory(&root, &dir, &recipe, &group, candidate.as_deref(), &output),
        Commands::Candidate {
            dir,
            recipe,
            run_id,
            attempt,
            output,
        } => candidates::candidate(&root, &dir, &recipe, &run_id, &attempt, &output),
        Commands::Verify {
            candidate,
            group,
            output,
        } => candidates::verify(&root, &candidate, &group, &output),
        Commands::VerifyCandidate {
            id,
            sha256,
            group,
            output,
        } => candidates::verify_candidate(&id, &sha256, group.as_deref(), output.as_deref()),
        Commands::Collect {
            candidate,
            receipts,
            output,
        } => candidates::collect(&root, &candidate, &receipts, &output),
        Commands::Publish {
            verified,
            candidate,
            destination,
            confirm,
            swift,
        } => publish::publish(&root, &verified, &candidate, destination, &confirm, &swift),
    }
}
