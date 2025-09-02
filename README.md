# Payments Engine

A simple toy payments engine that reads a series of transactions
from a CSV, updates client accounts, handles disputes and chargebacks, and then outputs the
state of clients accounts as a CSV.

## Usage

```shell
cargo run -- <input_csv> > accounts.csv
```

## Tests

```shell
cargo test
```

## Assumptions

The following assumptions have been made when designing and implementing this application:

* Disputes can only be made against Deposit transactions
* Disputes that would make the available balance go negative are not allowed, and therefore skipped
* Amounts passed in the CSV file must be positive
* Addition overflow, while probably unlikely, may happen when increasing available or held balance. When this would
  happen, the respective transaction is ignored.
    * In the case of total balance calculation (available + held), the application defaults to Decimal::MAX to prevent
      panicking
* (...)

## Design Choices

* Used newtypes for increased type safety (e.g. ClientId, TransactionId, Amount)
* (...)