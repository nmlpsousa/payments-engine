use crate::domain::{ClientAccountOutput, TransactionRow};
use crate::engine::PaymentsEngine;
use csv::{ReaderBuilder, Writer};
use std::io;

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
                    eprintln!("An error occurred while processing a transaction: {e:?}");
                }
            }
            Err(e) => {
                eprintln!("An error occurred while deserializing a row: {e}");
            }
        }
    }
}

pub fn print_account_records(
    engine: &PaymentsEngine,
    output: impl io::Write,
) -> Result<(), io::Error> {
    let client_accounts = engine.client_accounts();
    let mut writer = Writer::from_writer(output);
    for (client_id, account) in client_accounts {
        writer.serialize::<ClientAccountOutput>((client_id, account).into())?;
    }
    writer.flush()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::TransactionType::{Chargeback, Deposit, Dispute};
    use crate::domain::{
        Amount, ClientId, Transaction, TransactionId, TransactionStatus, TransactionType,
    };
    use rust_decimal::{dec, Decimal};
    use std::io::Cursor;

    fn create_test_csv(data: &str) -> Cursor<Vec<u8>> {
        Cursor::new(data.as_bytes().to_vec())
    }

    fn create_transaction(
        tx_type: TransactionType,
        client: u16,
        tx_id: u32,
        amount: Option<Decimal>,
    ) -> Transaction {
        Transaction {
            tx_type,
            client: ClientId::new(client),
            tx: TransactionId::new(tx_id),
            amount: amount.map(|a| Amount::new(a).unwrap()),
            tx_status: TransactionStatus::Pending,
        }
    }

    fn create_engine_with_account() -> PaymentsEngine {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::ONE));
        let _ = engine.process_transaction(deposit);

        engine
    }

    #[test]
    fn test_process_csv_valid_deposit() {
        let mut engine = PaymentsEngine::new();
        let csv_data = "type,client,tx,amount\ndeposit,1,1,1.0";
        let input = create_test_csv(csv_data);

        process_csv_transactions(&mut engine, input);

        let accounts = engine.client_accounts();
        assert_eq!(accounts.len(), 1);
        let account = accounts.get(&ClientId::new(1)).unwrap();
        assert_eq!(account.available_balance, Decimal::ONE);
    }

    #[test]
    fn test_process_csv_valid_withdrawal() {
        let mut engine = create_engine_with_account();
        let csv_data = "type,client,tx,amount\nwithdrawal,1,2,0.5";
        let input = create_test_csv(csv_data);

        process_csv_transactions(&mut engine, input);

        let accounts = engine.client_accounts();
        let account = accounts.get(&ClientId::new(1)).unwrap();
        assert_eq!(account.available_balance, dec!(0.5));
    }

    #[test]
    fn test_process_csv_multiple_transactions() {
        let mut engine = PaymentsEngine::new();
        let csv_data =
            "type,client,tx,amount\ndeposit,1,1,1.0\ndeposit,2,2,2.0\nwithdrawal,1,3,0.5";
        let input = create_test_csv(csv_data);

        process_csv_transactions(&mut engine, input);

        let accounts = engine.client_accounts();
        assert_eq!(accounts.len(), 2);

        let account1 = accounts.get(&ClientId::new(1)).unwrap();
        assert_eq!(account1.available_balance, dec!(0.5));

        let account2 = accounts.get(&ClientId::new(2)).unwrap();
        assert_eq!(account2.available_balance, dec!(2));
    }

    #[test]
    fn test_process_csv_dispute_resolve() {
        let mut engine = PaymentsEngine::new();
        let csv_data = "type,client,tx,amount\ndeposit,1,1,1.0\ndispute,1,1,\nresolve,1,1,";
        let input = create_test_csv(csv_data);

        process_csv_transactions(&mut engine, input);

        let accounts = engine.client_accounts();
        let account = accounts.get(&ClientId::new(1)).unwrap();
        assert_eq!(account.available_balance, Decimal::ONE);
        assert_eq!(account.held_balance, Decimal::ZERO);
    }

    #[test]
    fn test_process_csv_chargeback() {
        let mut engine = PaymentsEngine::new();
        let csv_data = "type,client,tx,amount\ndeposit,1,1,1.0\ndispute,1,1,\nchargeback,1,1,";
        let input = create_test_csv(csv_data);

        process_csv_transactions(&mut engine, input);

        let accounts = engine.client_accounts();
        let account = accounts.get(&ClientId::new(1)).unwrap();
        assert_eq!(account.available_balance, Decimal::ZERO);
        assert_eq!(account.held_balance, Decimal::ZERO);
        assert!(account.locked);
    }

    #[test]
    fn test_process_csv_empty_input() {
        let mut engine = PaymentsEngine::new();
        let csv_data = "";
        let input = create_test_csv(csv_data);

        process_csv_transactions(&mut engine, input);

        let accounts = engine.client_accounts();
        assert_eq!(accounts.len(), 0);
    }

    #[test]
    fn test_process_csv_headers_only() {
        let mut engine = PaymentsEngine::new();
        let csv_data = "type,client,tx,amount";
        let input = create_test_csv(csv_data);

        process_csv_transactions(&mut engine, input);

        let accounts = engine.client_accounts();
        assert_eq!(accounts.len(), 0);
    }

    #[test]
    fn test_process_csv_whitespace_trimming() {
        let mut engine = PaymentsEngine::new();
        let csv_data = "type,client,tx,amount\n  deposit  , 1 , 1 , 1.0  ";
        let input = create_test_csv(csv_data);

        process_csv_transactions(&mut engine, input);

        let accounts = engine.client_accounts();
        assert_eq!(accounts.len(), 1);
        let account = accounts.get(&ClientId::new(1)).unwrap();
        assert_eq!(account.available_balance, Decimal::ONE);
    }

    #[test]
    fn test_process_csv_missing_amount_for_deposit() {
        let mut engine = PaymentsEngine::new();
        let csv_data = "type,client,tx,amount\ndeposit,1,1,";
        let input = create_test_csv(csv_data);

        process_csv_transactions(&mut engine, input);

        let accounts = engine.client_accounts();
        assert_eq!(accounts.len(), 1);
        let account = accounts.get(&ClientId::new(1)).unwrap();
        assert_eq!(account.available_balance, Decimal::ZERO);
    }

    #[test]
    fn test_process_csv_invalid_transaction_type() {
        let mut engine = PaymentsEngine::new();
        let csv_data = "type,client,tx,amount\ninvalid,1,1,1.0";
        let input = create_test_csv(csv_data);

        process_csv_transactions(&mut engine, input);

        let accounts = engine.client_accounts();
        assert_eq!(accounts.len(), 0);
    }

    #[test]
    fn test_process_csv_negative_amount() {
        let mut engine = PaymentsEngine::new();
        let csv_data = "type,client,tx,amount\ndeposit,1,1,-1.0";
        let input = create_test_csv(csv_data);

        process_csv_transactions(&mut engine, input);

        let accounts = engine.client_accounts();
        assert_eq!(accounts.len(), 0);
    }

    #[test]
    fn test_process_csv_decimal_precision() {
        let mut engine = PaymentsEngine::new();
        let csv_data = "type,client,tx,amount\ndeposit,1,1,1.2345";
        let input = create_test_csv(csv_data);

        process_csv_transactions(&mut engine, input);

        let accounts = engine.client_accounts();
        let account = accounts.get(&ClientId::new(1)).unwrap();
        assert_eq!(account.available_balance, dec!(1.2345));
    }

    #[test]
    fn test_print_account_records_empty() {
        let engine = PaymentsEngine::new();
        let mut output = Vec::new();
        print_account_records(&engine, &mut output).unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.is_empty() || result == "client,available,held,total,locked\n");
    }

    #[test]
    fn test_print_account_records_single_account() {
        let mut engine = PaymentsEngine::new();
        let deposit = create_transaction(Deposit, 1, 1, Some(dec!(1.5)));
        engine.process_transaction(deposit).unwrap();

        let mut output = Vec::new();
        print_account_records(&engine, &mut output).unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("client,available,held,total,locked"));
        assert!(result.contains("1,1.5000,0.0000,1.5000,false"));
    }

    #[test]
    fn test_print_account_records_multiple_accounts() {
        let mut engine = PaymentsEngine::new();

        let transactions = vec![
            create_transaction(Deposit, 1, 1, Some(Decimal::ONE)),
            create_transaction(Deposit, 2, 2, Some(dec!(2.5))),
        ];

        for tx in transactions {
            engine.process_transaction(tx).unwrap();
        }

        let mut output = Vec::new();
        print_account_records(&engine, &mut output).unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("client,available,held,total,locked"));
        assert!(
            result.contains("1.0000,0.0000,1.0000,false")
                && result.contains("2.5000,0.0000,2.5000,false")
        );
    }

    #[test]
    fn test_print_account_records_locked_account() {
        let mut engine = PaymentsEngine::new();

        let transactions = vec![
            create_transaction(Deposit, 1, 1, Some(Decimal::ONE)),
            create_transaction(Dispute, 1, 1, None),
            create_transaction(Chargeback, 1, 1, None),
        ];

        for tx in transactions {
            engine.process_transaction(tx).unwrap();
        }

        let mut output = Vec::new();
        print_account_records(&engine, &mut output).unwrap();

        let result = String::from_utf8(output).unwrap();
        assert_eq!(
            result,
            "client,available,held,total,locked\n1,0.0000,0.0000,0.0000,true\n"
        );
    }

    #[test]
    fn test_end_to_end_processing() {
        let mut engine = PaymentsEngine::new();
        let csv_data = "type,client,tx,amount
deposit,1,1,1.0
deposit,2,2,2.0
deposit,1,3,2.0
withdrawal,1,4,1.5
dispute,1,1,
resolve,1,1,";

        let input = create_test_csv(csv_data);
        process_csv_transactions(&mut engine, input);

        let accounts = engine.client_accounts();
        assert_eq!(accounts.len(), 2);

        let account1 = accounts.get(&ClientId::new(1)).unwrap();
        assert_eq!(account1.available_balance, dec!(1.5));
        assert_eq!(account1.held_balance, Decimal::ZERO);
        assert!(!account1.locked);

        let account2 = accounts.get(&ClientId::new(2)).unwrap();
        assert_eq!(account2.available_balance, dec!(2));
        assert_eq!(account2.held_balance, Decimal::ZERO);
        assert!(!account2.locked);

        let mut output = Vec::new();
        print_account_records(&engine, &mut output).unwrap();

        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("client,available,held,total,locked"));
        assert!(
            result.contains("1.5000,0.0000,1.5000,false")
                && result.contains("2.0000,0.0000,2.0000,false")
        );
    }
}
