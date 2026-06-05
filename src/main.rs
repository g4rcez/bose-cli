use anyhow::Result;
use bose_cli::cli;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let args = cli::Args::parse();
    cli::run(args).await
}
