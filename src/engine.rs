use crate::domain::TransactionStatus::{ChargedBack, Disputed, Resolved, Settled};
use crate::domain::{ClientId, Transaction, TransactionId, TransactionType};
use crate::engine::ProcessingError::{
    InsufficientFunds, InvalidDispute, InvalidTransactionStatus, TransactionAlreadyDisputed,
    TransactionNotFound,
};
use rust_decimal::Decimal;
use std::collections::HashMap;
use TransactionType::{Chargeback, Deposit, Dispute, Resolve, Withdrawal};

#[derive(Debug, Clone, Default)]
pub struct ClientAccount {
    pub available_balance: Decimal,
    pub held_balance: Decimal,
    pub locked: bool,
}

impl ClientAccount {
    pub fn total(&self) -> Decimal {
        self.available_balance + self.held_balance
    }
}

#[derive(Debug)]
pub enum ProcessingError {
    MissingAmount,
    InsufficientFunds,
    AccountLocked,
    TransactionNotFound,
    TransactionAlreadyDisputed,
    InvalidTransactionStatus,
    InvalidDispute,
}

pub struct PaymentsEngine {
    clients: HashMap<ClientId, ClientAccount>,
    transaction_history: HashMap<TransactionId, Transaction>,
}

impl PaymentsEngine {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            transaction_history: HashMap::new(),
        }
    }

    pub fn process_transaction(&mut self, transaction: Transaction) -> Result<(), ProcessingError> {
        let client = self.clients.entry(transaction.client).or_default();

        if client.locked {
            return Err(ProcessingError::AccountLocked);
        }

        if transaction.tx_type.is_standard_transaction()
            && self.transaction_history.contains_key(&transaction.tx)
        {
            // Transaction was already processed, let's skip this
            return Ok(());
        }

        match transaction.tx_type {
            Deposit => self.process_deposit(transaction),
            Withdrawal => self.process_withdrawal(transaction),
            Dispute => self.process_dispute(transaction),
            Resolve => self.process_resolve(transaction),
            Chargeback => self.process_chargeback(transaction),
        }
    }

    fn process_deposit(&mut self, mut transaction: Transaction) -> Result<(), ProcessingError> {
        let amount = transaction.amount.ok_or(ProcessingError::MissingAmount)?;

        // Safe to unwrap as client has already been created in the main method
        let client = self.clients.get_mut(&transaction.client).unwrap();
        client.available_balance += amount.value();
        transaction.tx_status = Settled;

        self.transaction_history.insert(transaction.tx, transaction);

        Ok(())
    }

    fn process_withdrawal(&mut self, mut transaction: Transaction) -> Result<(), ProcessingError> {
        let amount = transaction.amount.ok_or(ProcessingError::MissingAmount)?;

        // Safe to unwrap as client has already been created in the main method
        let client = self.clients.get_mut(&transaction.client).unwrap();
        if client.available_balance < transaction.amount.unwrap().value() {
            return Err(InsufficientFunds);
        }

        client.available_balance -= amount.value();
        transaction.tx_status = Settled;

        self.transaction_history.insert(transaction.tx, transaction);

        Ok(())
    }

    fn process_dispute(&mut self, transaction: Transaction) -> Result<(), ProcessingError> {
        let original_tx = self
            .transaction_history
            .get_mut(&transaction.tx)
            .ok_or(TransactionNotFound)?;

        if transaction.client != original_tx.client {
            return Err(TransactionNotFound);
        }

        // Disputes are only possible against Deposit transactions
        if !matches!(original_tx.tx_type, Deposit) {
            return Err(InvalidDispute);
        }

        if !matches!(original_tx.tx_status, Settled) {
            // A dispute can only be opened on a transaction that hasn't had any other disputes
            return Err(TransactionAlreadyDisputed);
        }

        // Safe to unwrap as client is guaranteed to exist at this point
        let client = self.clients.get_mut(&transaction.client).unwrap();

        let original_amount = original_tx.amount.unwrap().value();
        if client.available_balance < original_amount {
            return Err(InsufficientFunds);
        }

        client.available_balance -= original_amount;
        client.held_balance += original_amount;

        original_tx.tx_status = Disputed;

        Ok(())
    }

    fn process_resolve(&mut self, transaction: Transaction) -> Result<(), ProcessingError> {
        let original_tx = self
            .transaction_history
            .get_mut(&transaction.tx)
            .ok_or(TransactionNotFound)?;

        if original_tx.client != transaction.client {
            return Err(TransactionNotFound);
        }

        if !matches!(original_tx.tx_status, Disputed) {
            return Err(InvalidTransactionStatus);
        }

        // Safe to unwrap as client is guaranteed to exist at this point
        let client = self.clients.get_mut(&transaction.client).unwrap();

        let original_amount = original_tx.amount.unwrap().value();
        client.available_balance += original_amount;
        client.held_balance -= original_amount;
        original_tx.tx_status = Resolved;

        Ok(())
    }

    fn process_chargeback(&mut self, transaction: Transaction) -> Result<(), ProcessingError> {
        let original_tx = self
            .transaction_history
            .get_mut(&transaction.tx)
            .ok_or(TransactionNotFound)?;

        if original_tx.client != transaction.client {
            return Err(TransactionNotFound);
        }

        if !matches!(original_tx.tx_status, Disputed) {
            return Err(InvalidTransactionStatus);
        }

        // Safe to unwrap as client is guaranteed to exist at this point
        let client = self.clients.get_mut(&transaction.client).unwrap();

        let original_amount = original_tx.amount.unwrap().value();
        client.held_balance -= original_amount;
        client.locked = true;
        original_tx.tx_status = ChargedBack;

        Ok(())
    }

    pub fn client_accounts(&self) -> &HashMap<ClientId, ClientAccount> {
        &self.clients
    }
}
