use crate::domain::TransactionStatus::Pending;
use crate::engine::ClientAccount;
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::convert::TryFrom;
use std::fmt::{Display, Formatter};

#[derive(Serialize, Deserialize, Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[serde(transparent)]
pub struct ClientId(u16);

impl ClientId {
    pub fn new(val: u16) -> Self {
        Self(val)
    }

    pub fn value(&self) -> u16 {
        self.0
    }
}

impl Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Deserialize, Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[serde(transparent)]
pub struct TransactionId(u32);

impl TransactionId {
    pub fn new(val: u32) -> Self {
        Self(val)
    }

    pub fn value(&self) -> u32 {
        self.0
    }
}

#[derive(Deserialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

impl TransactionType {
    pub fn is_standard_transaction(&self) -> bool {
        matches!(self, TransactionType::Deposit | TransactionType::Withdrawal)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Amount(Decimal);

impl Amount {
    pub fn new(val: Decimal) -> Result<Self, AmountError> {
        if val > Decimal::ZERO {
            Ok(Self(val))
        } else {
            Err(AmountError::NonPositiveAmount(val))
        }
    }

    pub fn value(&self) -> Decimal {
        self.0
    }
}

#[derive(Debug, Clone)]
pub enum AmountError {
    NonPositiveAmount(Decimal),
}

impl Display for AmountError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            AmountError::NonPositiveAmount(dec) => write!(f, "Amount must be positive: {dec}"),
        }
    }
}

impl TryFrom<Decimal> for Amount {
    type Error = AmountError;

    fn try_from(value: Decimal) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let decimal = <Decimal as Deserialize>::deserialize(deserializer)?;
        Amount::new(decimal).map_err(serde::de::Error::custom)
    }
}

impl Serialize for Amount {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{:.4}", self.value()))
    }
}

#[derive(Deserialize, Debug)]
pub struct TransactionRow {
    #[serde(rename = "type")]
    pub tx_type: TransactionType,
    pub client: ClientId,
    pub tx: TransactionId,
    pub amount: Option<Amount>,
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub tx_type: TransactionType,
    pub client: ClientId,
    pub tx: TransactionId,
    pub amount: Option<Amount>,
    pub tx_status: TransactionStatus,
}

impl From<TransactionRow> for Transaction {
    fn from(value: TransactionRow) -> Self {
        Self {
            tx_type: value.tx_type,
            client: value.client,
            tx: value.tx,
            amount: value.amount,
            tx_status: Pending,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TransactionStatus {
    Pending,
    Settled,
    Disputed,
    Resolved,
    ChargedBack,
}

#[derive(Debug, Serialize)]
pub struct ClientAccountOutput {
    client: ClientId,
    #[serde(serialize_with = "serialize_decimal_with_precision_4")]
    available: Decimal,
    #[serde(serialize_with = "serialize_decimal_with_precision_4")]
    held: Decimal,
    #[serde(serialize_with = "serialize_decimal_with_precision_4")]
    total: Decimal,
    locked: bool,
}

impl From<(&ClientId, &ClientAccount)> for ClientAccountOutput {
    fn from((client_id, client_account): (&ClientId, &ClientAccount)) -> Self {
        Self {
            client: *client_id,
            available: client_account.available_balance,
            held: client_account.held_balance,
            total: client_account.total(),
            locked: client_account.locked,
        }
    }
}

fn serialize_decimal_with_precision_4<S>(
    decimal: &Decimal,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("{decimal:.4}"))
}
