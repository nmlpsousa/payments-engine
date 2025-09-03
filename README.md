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
* Disputes that would make the available balance go negative are not allowed, and therefore ignored
* Disputes, Resolves and Chargebacks require both the correct `ClientId` and `TransactionId`. If the provided `ClientId`
  does
  not match the original transaction, then this transaction is ignored
* Disputes can be opened multiple times against the same transaction, provided all prior disputes have been resolved
* Amounts passed in the CSV file must be positive
* Addition overflow, while probably unlikely, may happen when increasing available or held balance. When this would
  happen, the respective transaction is ignored.
    * In the case of total balance calculation (available + held), the application defaults to `Decimal::MAX` to prevent
      panicking
* Invalid CSV rows are ignored

## Design Choices

### Concurrency

For the scope of this exercise, I chose to keep the application single-threaded to maintain simplicity and focus on core
functionality.

If this code were integrated into a web server requiring parallel processing, we would need to introduce synchronization
primitives such as `Mutex` or `RwLock` to ensure thread safety when accessing shared state.

A potential optimization would be to partition transactions by `ClientId` and distribute them across multiple worker
threads. Since transactions only affect their associated client account, we only need to guarantee processing order
within each `ClientId` - transactions for different clients can be processed concurrently without conflicts.

This approach would provide horizontal scalability while maintaining data consistency.

### Idempotency

Standard transactions (deposits and withdrawals) are idempotent based on transaction ID. If the same deposit or
withdrawal transaction ID appears multiple times in the file, only the first occurrence will be processed while
subsequent
duplicates are ignored. This approach enables safe retries without risking duplicate side effects or double-processing
of funds.

### Type Safety

Used newtypes for increased type safety and clarity.

This way instead of passing `u16`, `u32` and `Decimal` around, it is clear when a function expects a u16 that represents
a
ClientId, for instance.

* `ClientId(u16)`
* `TransactionId(u32)`
* `Amount(Decimal)`
    * Guaranteed to be a positive `Decimal`

### Error Handling

The application will skip processing invalid rows, whether due to formatting issues or incorrect data. Only critical failures, such as being unable to read the input file or write to stdout, will cause the application to panic.

## AI Policy

Used ChatGPT for general questions, and for help on how to use some functionality of crates such as `serde`
and `rust_decimal`.

Example prompts:

* What is the best way to serialize a `Decimal` from the `rust_decimal` crate with precision 4, using `serde`?
* In the context of a payments engine, should disputes be allowed on the same transaction multiple times?
* What are the pros and cons between implementing newtypes vs type aliases?
