use anyhow::Context;
use clap::Parser;
use payments_engine::csv;
use payments_engine::engine::PaymentsEngine;
use std::fs::File;
use std::io::stdout;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    pub csv_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    let file = File::open(args.csv_path).context("Failed to open input file")?;

    let mut engine = PaymentsEngine::new();
    csv::process_csv_transactions(&mut engine, file);
    csv::print_account_records(&engine, stdout())?;

    Ok(())
}
