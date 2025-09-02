use crate::domain::{ClientAccountOutput, TransactionRow};
use crate::engine::PaymentsEngine;
use csv::{ReaderBuilder, Writer};
use std::io;
use std::io::stdout;

pub fn process_csv_transactions(engine: &mut PaymentsEngine, input: impl io::Read) {
    let mut csv_reader = ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(input);

    for result in csv_reader.deserialize::<TransactionRow>() {
        match result {
            Ok(transaction) => {
                if let Err(e) = engine.process_transaction(transaction.into()) {
                    eprintln!("An error occurred while processing a transaction: {:?}", e);
                }
            }
            Err(e) => {
                eprintln!("An error occurred while deserializing a row: {}", e);
            }
        }
    }
}

pub fn print_account_records(engine: &PaymentsEngine) -> Result<(), io::Error> {
    let client_accounts = engine.client_accounts();
    let mut writer = Writer::from_writer(stdout());
    for (client_id, account) in client_accounts {
        writer.serialize::<ClientAccountOutput>((client_id, account).into())?;
    }
    writer.flush()?;

    Ok(())
}
