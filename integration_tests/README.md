# Tari integration cucumber tests

## Prerequisites

- Install `Node.js` (_Currently, LTS version 12.22.X is recommended for wallet FFI compatibility_)

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

- To run all tests and generate the reports (\*nix):

  ```
  ./run-tests.sh
  ```

- To run a specific test, add `-- --name <REGEXP>` to the command line:

  ```shell
  # Runs a specific test
  npm test -- --name "Simple block sync"
  ```

- To run tests with specific tags, e.g. `critical`, add `-- --tags @<EXPRESSION>` to the command line.

  ```shell
  # Runs all critical tests
  npm test -- --tags "@critical"
  ./run-tests.sh --tags "@critical"

    # Runs all critical tests base node tests
  npm test -- --tags "@critical AND @base-node"
  ./run-tests.sh --tags "@critical AND @base-node"

   # Runs all critical tests, but not @long-running and not @tbroken
  npm test -- --tags "@critical and not @long-running and not @broken"
  ```

- To run the wallet FFI tests, add `--profile ci` or `--profile none` to the command line (_take note of the node
  version requirements_) and take note of the profile definition in `cucumber.js`.

  ```shell
  # Runs a specific FFI test
  npm test -- --profile ci --name "My wallet FFI test scenario name"
  # Runs a complete set of FFI tests
  npm test -- --profile none --tags "@wallet-ffi"
  ```

- Runs all @critical tests, but not @long-running

  ```
  npm test -- --tags "@critical and not @long-running"
  ```

- See `npm test -- --help` for more options.

## Notes

In Windows, running the Tari executables in debug mode fails with a stack overflow, thus Windows users must
run in release mode. See command line options in `baseNodeProcess.js`, `mergeMiningProxyProcess.js`
and `walletProcess.js`.

## Code Contribution

[Prettier](https://prettier.io/) is used for JS code formatting. To ensure that your code is correctly
formatted, run the following to format or check your code in-place:

- `npm run fmt`
- `npm run check-fmt`

Alternatively, use a prettier plugin for your favourite IDE.

[ESLint](https://eslint.org) is used to statically analyzes the code to quickly find problems. To ensure your code
conforms to the linting standard, run the following to fix or check your code in-place:

- `npm run lint-fix`
- `npm run lint`
