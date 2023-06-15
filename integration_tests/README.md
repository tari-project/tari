# Tari integration cucumber tests

## Procedure to run

In its simplest form you can run the tests from the project route with `cargo test --release --test cucumber`

- To run a specific test, add `-- --name <Test Name>` to the command line

  ```shell
  # Runs a specific test
  cargo test --release --test cucumber -- --name "Basic connectivity between 2 nodes"

  # You can also use the short option -n
  cargo test --release --test cucumber -- -n "Basic connectivity between 2 nodes"
  ```

- To run tests with specific tags, e.g. `critical`, add `-- --tags @<EXPRESSION>` to the command line.

  ```shell
  # Runs all critical tests
  cargo test --release --test cucumber -- --tags "@critical"

  # Runs all critical tests base node tests
  cargo test --release --test cucumber -- --tags "@critical AND @base-node"

  # Runs all critical tests, but not @long-running and not @tbroken
  cargo test --release --test cucumber -- --tags "@critical and not @long-running and not @broken"

  # You can also use the short option -t
  cargo test --release --test cucumber -- -t "@critical"
  ```

- To run a specific file or files add `-- --input glob` to the command line

  ```shell
  # Runs all files matching glob
  cargo test --release --test cucumber -- --input "tests/features/Wallet*"

  # If you want to run a single file make the glob specific to the filename
  cargo test --release --test cucumber -- --input "tests/features/WalletTransactions*"

  # You can also use the short option -i
  cargo test --release --test cucumber -- -i "tests/features/WalletTransactions*"
  ```

## Notes

This suite is still a work in progress. We have scenarios marked `@broken` that are known broken and require fixing.
We also have scenarios commented out entirely. These scenarios are being reactivated and having any missing steps implemented
on a "when we have time" basis.

## CI

Every PR will run this suite but only on `@critical` tags. Each night the rest of the suite will run except `@long-running`.
Once a week over the weekend `@long-running` scenarios will run.