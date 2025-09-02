mod domain;
mod engine;

use crate::domain::ClientAccountOutput;
use crate::engine::PaymentsEngine;
use anyhow::{anyhow, Context};
use csv::{ReaderBuilder, Writer};
use domain::TransactionRow;
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

    let mut csv_reader = ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(file);

    let mut payments_engine = PaymentsEngine::new();

    for result in csv_reader.deserialize::<TransactionRow>() {
        match result {
            Ok(transaction) => {
                if let Err(e) = payments_engine.process_transaction(transaction.into()) {
                    eprintln!("An error occurred while processing a transaction: {:?}", e);
                }
            }
            Err(e) => {
                eprintln!("An error occurred while deserializing a row: {}", e);
            }
        }
    }

    let client_accounts = payments_engine.client_accounts();
    let mut writer = Writer::from_writer(stdout());
    for (client_id, account) in client_accounts {
        writer.serialize::<ClientAccountOutput>((client_id, account).into())?;
    }
    writer.flush()?;

    Ok(())
}
