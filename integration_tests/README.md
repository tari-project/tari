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
  ./run-tests.sh
  ```
  or on Windows
  ```
  node_modules\.bin\cucumber-js
  ```

- To run a specific test, add `--name <REGEXP>` to the command line.    

- To run specific tests, e.g. `critical`, add `--tags @<EXPRESSION>` to the command line.

- Examples:
  ```shell
   # Runs all critical tests 
   ./run-tests.sh --tags "@critical"
  
   # Runs all critical tests, but not @long-running
   ./run-tests.sh --tags "@critical and not @long-running" 
   ```

- See `--help` for more options.

## Notes

- In Windows, running the Tari executables in debug mode fails with a stack overflow, thus Windows users must 
  run in release mode. See command line options in `baseNodeProcess.js`, `mergeMiningProxyProcess.js` 
  and `walletProcess.js`.
