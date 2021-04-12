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

- To run all tests and generate the reports (\*nix):

  ```
  ./run-tests.sh
  ```

- To run a specific test, add `-- --name <REGEXP>` to the command line:

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

## Code Contribution

[Prettier](https://prettier.io/) is used for JS code formatting. To ensure that your code is correctly
formatted, run the following to format or check your code in-place:

- Enforce \*nix style line endings, i.e `lf` only:
  - `npm run fmt`
  - `npm run check-fmt`
- Allow prevalent line endings, i.e. `lf`, `crlf` or `cr` (_this is useful if your git is configured to correctly manage
  all line endings_):
  - `npm run fmt-auto`
  - `npm run check-fmt-auto`

Alternatively, use a prettier plugin for your favourite IDE.
