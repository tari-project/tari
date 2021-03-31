# Tari integration cucumber tests

## Prerequisites

- Install `Node.js`

- Open terminal in the `tari-project\integration_tests` folder and run
  ```
  npm install
  ```

## Procedure to run

- Open terminal in the `tari-project\integration_tests` folder

- Before running tests, you'll need to run `npm install`

  ```
  npm install
  ```

- To run all tests:

  ```
  npm test
  ```

- To run all tests and generate the reports:

  ```
  ./run-tests.sh
  ```

- To run a specific test, add `-- --name <REGEXP>` to the command line.

```shell
  # eg: run a specific test
  npm test -- --name "Simple block sync"
```

- To run tests with specific tags, e.g. `critical`, add `-- --tags @<EXPRESSION>` to the command line.

  ```shell
  # Runs all critical tests
  npm test -- --tags "@critical"

  # Runs all @critical tests, but not @long-running
  npm test -- --tags "@critical and not @long-running"
  ```

- See `npm test -- --help` for more options.

## Notes

- In Windows, running the Tari executables in debug mode fails with a stack overflow, thus Windows users must
  run in release mode. See command line options in `baseNodeProcess.js`, `mergeMiningProxyProcess.js`
  and `walletProcess.js`.
