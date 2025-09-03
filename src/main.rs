use anyhow::{anyhow, Context};
use payments_engine::csv;
use payments_engine::engine::PaymentsEngine;
use std::env;
use std::fs::File;
use std::io::stdout;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        return Err(anyhow!("Unexpected number of arguments passed"));
    }

    let csv_filename = &args[1];
    let file = File::open(csv_filename).context("Failed to open input file")?;

    let mut engine = PaymentsEngine::default();
    csv::process_csv_transactions(&mut engine, file);
    csv::print_account_records(&engine, stdout())?;

    Ok(())
}
