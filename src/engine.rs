use crate::domain::TransactionStatus::{ChargedBack, Disputed, Resolved, Settled};
use crate::domain::{ClientId, Transaction, TransactionId, TransactionType};
use crate::engine::ProcessingError::{
    BalanceOverflow, InsufficientFunds, InvalidDispute, InvalidTransactionStatus, MissingAmount,
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
        self.available_balance
            .checked_add(self.held_balance)
            .unwrap_or(Decimal::MAX)
    }
}

#[derive(Debug, PartialEq)]
pub enum ProcessingError {
    MissingAmount,
    InsufficientFunds,
    BalanceOverflow,
    AccountLocked,
    TransactionNotFound,
    InvalidTransactionStatus,
    InvalidDispute,
}

pub struct PaymentsEngine {
    clients: HashMap<ClientId, ClientAccount>,
    transaction_history: HashMap<TransactionId, Transaction>,
}

impl Default for PaymentsEngine {
    fn default() -> Self {
        Self::new()
    }
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
        client.available_balance = client
            .available_balance
            .checked_add(amount.value())
            .ok_or(BalanceOverflow)?;
        transaction.tx_status = Settled;

        self.transaction_history.insert(transaction.tx, transaction);

        Ok(())
    }

    fn process_withdrawal(&mut self, mut transaction: Transaction) -> Result<(), ProcessingError> {
        let amount = transaction.amount.ok_or(ProcessingError::MissingAmount)?;

        // Safe to unwrap as client has already been created in the main method
        let client = self.clients.get_mut(&transaction.client).unwrap();
        if client.available_balance < amount.value() {
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

        // A dispute can only be opened on a transaction that is settled, or that has had disputes that have since been resolved
        if !matches!(original_tx.tx_status, Settled | Resolved) {
            return Err(InvalidDispute);
        }

        // Safe to unwrap as client is guaranteed to exist at this point
        let client = self.clients.get_mut(&transaction.client).unwrap();

        let original_amount = original_tx.amount.ok_or(MissingAmount)?.value();
        if client.available_balance < original_amount {
            return Err(InsufficientFunds);
        }

        client.held_balance = client
            .held_balance
            .checked_add(original_amount)
            .ok_or(BalanceOverflow)?;
        client.available_balance -= original_amount;

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

        let original_amount = original_tx.amount.ok_or(MissingAmount)?.value();
        client.available_balance = client
            .available_balance
            .checked_add(original_amount)
            .ok_or(BalanceOverflow)?;
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

        let original_amount = original_tx.amount.ok_or(MissingAmount)?.value();
        client.held_balance -= original_amount;
        client.locked = true;
        original_tx.tx_status = ChargedBack;

        Ok(())
    }

    pub fn client_accounts(&self) -> &HashMap<ClientId, ClientAccount> {
        &self.clients
    }

    #[cfg(test)]
    pub fn lock_account(&mut self, client_id: ClientId) {
        if let Some(account) = self.clients.get_mut(&client_id) {
            account.locked = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Amount, TransactionStatus};
    use rust_decimal::dec;

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

    #[test]
    fn test_payments_engine_new() {
        let engine = PaymentsEngine::new();
        assert!(engine.clients.is_empty());
        assert!(engine.transaction_history.is_empty());
        assert!(engine.client_accounts().is_empty());
    }

    #[test]
    fn test_client_account_total() {
        let mut account = ClientAccount::default();
        assert_eq!(account.total(), Decimal::ZERO);

        account.available_balance = Decimal::TEN;
        account.held_balance = dec!(5);
        assert_eq!(account.total(), dec!(15));
    }

    #[test]
    fn test_client_account_default() {
        let account = ClientAccount::default();
        assert_eq!(account.available_balance, Decimal::ZERO);
        assert_eq!(account.held_balance, Decimal::ZERO);
        assert!(!account.locked);
    }

    #[test]
    fn test_deposit_happy_path() {
        let mut engine = PaymentsEngine::new();
        let transaction = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));

        let result = engine.process_transaction(transaction);

        assert!(result.is_ok());
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::TEN);
        assert_eq!(client_account.held_balance, Decimal::ZERO);
        assert_eq!(client_account.total(), Decimal::TEN);
        assert!(!client_account.locked);
    }

    #[test]
    fn test_deposit_multiple_deposits_same_client() {
        let mut engine = PaymentsEngine::new();
        let tx1 = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        let tx2 = create_transaction(Deposit, 1, 2, Some(dec!(5)));

        engine.process_transaction(tx1).unwrap();
        engine.process_transaction(tx2).unwrap();

        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, dec!(15));
        assert_eq!(client_account.total(), dec!(15));
    }

    #[test]
    fn test_deposit_missing_amount() {
        let mut engine = PaymentsEngine::new();
        let transaction = create_transaction(Deposit, 1, 1, None);

        let result = engine.process_transaction(transaction);

        assert_eq!(result, Err(ProcessingError::MissingAmount));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::ZERO);
        assert_eq!(client_account.held_balance, Decimal::ZERO);
    }

    #[test]
    fn test_deposit_new_client_creation() {
        let mut engine = PaymentsEngine::new();
        let transaction = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));

        engine.process_transaction(transaction).unwrap();

        assert_eq!(engine.clients.len(), 1);
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::TEN);
        assert_eq!(client_account.held_balance, Decimal::ZERO);
        assert!(!client_account.locked);
    }

    #[test]
    fn test_deposit_decimal_precision() {
        let mut engine = PaymentsEngine::new();
        let amount = dec!(12.3456);
        let transaction = create_transaction(Deposit, 1, 1, Some(amount));

        engine.process_transaction(transaction).unwrap();

        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, amount);
    }

    #[test]
    fn test_deposit_large_amount() {
        let mut engine = PaymentsEngine::new();
        let large_amount = Decimal::MAX;
        let transaction = create_transaction(Deposit, 1, 1, Some(large_amount));

        let result = engine.process_transaction(transaction);

        assert!(result.is_ok());
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, large_amount);
    }

    #[test]
    fn test_deposit_balance_overflow() {
        let mut engine = PaymentsEngine::new();
        let large_amount = Decimal::MAX;
        let tx1 = create_transaction(Deposit, 1, 1, Some(large_amount));
        let tx2 = create_transaction(Deposit, 1, 2, Some(dec!(1)));

        let result1 = engine.process_transaction(tx1);
        let result2 = engine.process_transaction(tx2);

        assert!(result1.is_ok());
        assert_eq!(result2, Err(BalanceOverflow));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, large_amount);
    }

    #[test]
    fn test_deposit_duplicate_deposit_ignored() {
        let mut engine = PaymentsEngine::new();
        let tx1 = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));

        engine.process_transaction(tx1.clone()).unwrap();
        let result = engine.process_transaction(tx1);

        assert!(result.is_ok());
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::TEN);
    }

    #[test]
    fn test_deposit_to_locked_account() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        engine.process_transaction(deposit).unwrap();
        engine.lock_account(ClientId::new(1));

        let withdrawal = create_transaction(Deposit, 1, 2, Some(dec!(5)));
        let result = engine.process_transaction(withdrawal);

        assert_eq!(result, Err(ProcessingError::AccountLocked));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::TEN);
    }

    #[test]
    fn test_withdrawal_happy_path() {
        let mut engine = PaymentsEngine::new();

        // Setup: deposit first
        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        engine.process_transaction(deposit).unwrap();

        let withdrawal = create_transaction(Withdrawal, 1, 2, Some(dec!(5)));
        let result = engine.process_transaction(withdrawal);

        assert!(result.is_ok());
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, dec!(5));
        assert_eq!(client_account.total(), dec!(5));
    }

    #[test]
    fn test_withdrawal_exact_balance() {
        let mut engine = PaymentsEngine::new();

        // Setup: deposit
        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        engine.process_transaction(deposit).unwrap();

        let withdrawal = create_transaction(Withdrawal, 1, 2, Some(Decimal::TEN));
        let result = engine.process_transaction(withdrawal);

        assert!(result.is_ok());
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::ZERO);
        assert_eq!(client_account.total(), Decimal::ZERO);
    }

    #[test]
    fn test_withdrawal_insufficient_funds() {
        let mut engine = PaymentsEngine::new();

        // Setup: deposit small amount
        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::ONE));
        engine.process_transaction(deposit).unwrap();

        let withdrawal = create_transaction(Withdrawal, 1, 2, Some(Decimal::TEN));
        let result = engine.process_transaction(withdrawal);

        assert_eq!(result, Err(ProcessingError::InsufficientFunds));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::ONE);
    }

    #[test]
    fn test_withdrawal_missing_amount() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        engine.process_transaction(deposit).unwrap();

        let withdrawal = create_transaction(Withdrawal, 1, 2, None);
        let result = engine.process_transaction(withdrawal);

        assert_eq!(result, Err(ProcessingError::MissingAmount));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::TEN);
    }

    #[test]
    fn test_withdrawal_from_locked_account() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        engine.process_transaction(deposit).unwrap();
        engine.lock_account(ClientId::new(1));

        let withdrawal = create_transaction(Withdrawal, 1, 2, Some(dec!(5)));
        let result = engine.process_transaction(withdrawal);

        assert_eq!(result, Err(ProcessingError::AccountLocked));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::TEN);
    }

    #[test]
    fn test_withdrawal_duplicate_transaction_id_ignored() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(dec!(20)));
        engine.process_transaction(deposit).unwrap();

        let withdrawal = create_transaction(Withdrawal, 1, 2, Some(dec!(5)));
        engine.process_transaction(withdrawal.clone()).unwrap();

        let result = engine.process_transaction(withdrawal);

        assert!(result.is_ok());
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, dec!(15));
    }

    #[test]
    fn test_withdrawal_from_zero_balance() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        engine.process_transaction(deposit).unwrap();
        let withdrawal1 = create_transaction(Withdrawal, 1, 2, Some(Decimal::TEN));
        engine.process_transaction(withdrawal1).unwrap();

        let withdrawal2 = create_transaction(Withdrawal, 1, 3, Some(Decimal::ONE));
        let result = engine.process_transaction(withdrawal2);

        assert_eq!(result, Err(ProcessingError::InsufficientFunds));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::ZERO);
    }

    #[test]
    fn test_dispute_valid_deposit_happy_path() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        engine.process_transaction(deposit).unwrap();

        let dispute = create_transaction(Dispute, 1, 1, None);
        let result = engine.process_transaction(dispute);

        assert!(result.is_ok());
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::ZERO);
        assert_eq!(client_account.held_balance, Decimal::TEN);
        assert_eq!(client_account.total(), Decimal::TEN);

        let original_tx = engine
            .transaction_history
            .get(&TransactionId::new(1))
            .unwrap();
        assert!(matches!(original_tx.tx_status, TransactionStatus::Disputed));
    }

    #[test]
    fn test_dispute_transaction_not_found() {
        let mut engine = PaymentsEngine::new();

        let dispute = create_transaction(Dispute, 1, 1, None);
        let result = engine.process_transaction(dispute);

        assert_eq!(result, Err(ProcessingError::TransactionNotFound));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::ZERO);
        assert_eq!(client_account.held_balance, Decimal::ZERO);
    }

    #[test]
    fn test_dispute_wrong_client() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        engine.process_transaction(deposit).unwrap();

        let dispute = create_transaction(Dispute, 2, 1, None);
        let result = engine.process_transaction(dispute);

        assert_eq!(result, Err(ProcessingError::TransactionNotFound));

        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::TEN);
        assert_eq!(client_account.held_balance, Decimal::ZERO);
    }

    #[test]
    fn test_dispute_withdrawal_invalid() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(dec!(20)));
        let withdrawal = create_transaction(Withdrawal, 1, 2, Some(Decimal::TEN));
        engine.process_transaction(deposit).unwrap();
        engine.process_transaction(withdrawal).unwrap();

        let dispute = create_transaction(Dispute, 1, 2, None);
        let result = engine.process_transaction(dispute);

        assert_eq!(result, Err(ProcessingError::InvalidDispute));

        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::TEN);
        assert_eq!(client_account.held_balance, Decimal::ZERO);
    }

    #[test]
    fn test_dispute_already_disputed_transaction() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        let dispute1 = create_transaction(Dispute, 1, 1, None);
        engine.process_transaction(deposit).unwrap();
        engine.process_transaction(dispute1).unwrap();

        let dispute2 = create_transaction(Dispute, 1, 1, None);
        let result = engine.process_transaction(dispute2);

        assert_eq!(result, Err(ProcessingError::InvalidDispute));

        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::ZERO);
        assert_eq!(client_account.held_balance, Decimal::TEN);
    }

    #[test]
    fn test_dispute_resolved_transaction() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        let dispute = create_transaction(Dispute, 1, 1, None);
        let resolve = create_transaction(Resolve, 1, 1, None);
        engine.process_transaction(deposit).unwrap();
        engine.process_transaction(dispute).unwrap();
        engine.process_transaction(resolve).unwrap();

        let dispute2 = create_transaction(Dispute, 1, 1, None);
        let result = engine.process_transaction(dispute2);

        assert!(result.is_ok());

        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::ZERO);
        assert_eq!(client_account.held_balance, Decimal::TEN);
    }

    #[test]
    fn test_dispute_chargedback_transaction() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        let dispute = create_transaction(Dispute, 1, 1, None);
        let chargeback = create_transaction(Chargeback, 1, 1, None);
        engine.process_transaction(deposit).unwrap();
        engine.process_transaction(dispute).unwrap();
        engine.process_transaction(chargeback).unwrap();

        let dispute2 = create_transaction(Dispute, 1, 1, None);
        let result = engine.process_transaction(dispute2);

        assert_eq!(result, Err(ProcessingError::AccountLocked));
    }

    #[test]
    fn test_dispute_insufficient_funds_for_dispute() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        let withdrawal = create_transaction(Withdrawal, 1, 2, Some(dec!(8)));
        engine.process_transaction(deposit).unwrap();
        engine.process_transaction(withdrawal).unwrap();

        let dispute = create_transaction(Dispute, 1, 1, None);
        let result = engine.process_transaction(dispute);

        assert_eq!(result, Err(ProcessingError::InsufficientFunds));

        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, dec!(2));
        assert_eq!(client_account.held_balance, Decimal::ZERO);
    }

    #[test]
    fn test_dispute_from_locked_account() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        engine.process_transaction(deposit).unwrap();
        engine.lock_account(ClientId::new(1));

        let dispute = create_transaction(Dispute, 1, 1, None);
        let result = engine.process_transaction(dispute);

        assert_eq!(result, Err(ProcessingError::AccountLocked));

        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::TEN);
        assert_eq!(client_account.held_balance, Decimal::ZERO);
    }

    #[test]
    fn test_resolve_happy_path() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        let dispute = create_transaction(Dispute, 1, 1, None);
        let resolve = create_transaction(Resolve, 1, 1, None);
        engine.process_transaction(deposit).unwrap();
        engine.process_transaction(dispute).unwrap();
        let result = engine.process_transaction(resolve);

        assert!(result.is_ok());
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::TEN);
        assert_eq!(client_account.total(), Decimal::TEN);
    }

    #[test]
    fn test_resolve_undisputed_transaction() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        let resolve = create_transaction(Resolve, 1, 1, None);
        engine.process_transaction(deposit).unwrap();
        let result = engine.process_transaction(resolve);

        assert_eq!(result, Err(ProcessingError::InvalidTransactionStatus));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::TEN);
        assert_eq!(client_account.total(), Decimal::TEN);
    }

    #[test]
    fn test_resolve_transaction_not_found() {
        let mut engine = PaymentsEngine::new();

        let resolve = create_transaction(Resolve, 1, 1, None);
        let result = engine.process_transaction(resolve);

        assert_eq!(result, Err(ProcessingError::TransactionNotFound));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::ZERO);
        assert_eq!(client_account.total(), Decimal::ZERO);
    }

    #[test]
    fn test_resolve_invalid_client() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        let dispute = create_transaction(Dispute, 1, 1, None);
        let resolve = create_transaction(Resolve, 2, 1, None);
        engine.process_transaction(deposit).unwrap();
        engine.process_transaction(dispute).unwrap();
        let result = engine.process_transaction(resolve);

        assert_eq!(result, Err(TransactionNotFound));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::ZERO);
        assert_eq!(client_account.held_balance, Decimal::TEN);
        assert_eq!(client_account.total(), Decimal::TEN);
    }

    #[test]
    fn test_chargeback_happy_path() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        let dispute = create_transaction(Dispute, 1, 1, None);
        let resolve = create_transaction(Chargeback, 1, 1, None);
        engine.process_transaction(deposit).unwrap();
        engine.process_transaction(dispute).unwrap();
        let result = engine.process_transaction(resolve);

        assert!(result.is_ok());
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::ZERO);
        assert_eq!(client_account.total(), Decimal::ZERO);
        assert!(client_account.locked);
    }

    #[test]
    fn test_chargeback_undisputed_transaction() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        let resolve = create_transaction(Chargeback, 1, 1, None);
        engine.process_transaction(deposit).unwrap();
        let result = engine.process_transaction(resolve);

        assert_eq!(result, Err(ProcessingError::InvalidTransactionStatus));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::TEN);
        assert_eq!(client_account.total(), Decimal::TEN);
        assert!(!client_account.locked);
    }

    #[test]
    fn test_chargeback_transaction_not_found() {
        let mut engine = PaymentsEngine::new();

        let resolve = create_transaction(Chargeback, 1, 1, None);
        let result = engine.process_transaction(resolve);

        assert_eq!(result, Err(ProcessingError::TransactionNotFound));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::ZERO);
        assert_eq!(client_account.total(), Decimal::ZERO);
        assert!(!client_account.locked);
    }

    #[test]
    fn test_chargeback_invalid_client() {
        let mut engine = PaymentsEngine::new();

        let deposit = create_transaction(Deposit, 1, 1, Some(Decimal::TEN));
        let dispute = create_transaction(Dispute, 1, 1, None);
        let resolve = create_transaction(Chargeback, 2, 1, None);
        engine.process_transaction(deposit).unwrap();
        engine.process_transaction(dispute).unwrap();
        let result = engine.process_transaction(resolve);

        assert_eq!(result, Err(TransactionNotFound));
        let client_account = engine.clients.get(&ClientId::new(1)).unwrap();
        assert_eq!(client_account.available_balance, Decimal::ZERO);
        assert_eq!(client_account.held_balance, Decimal::TEN);
        assert_eq!(client_account.total(), Decimal::TEN);
        assert!(!client_account.locked);
    }
}
