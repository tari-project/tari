# Tari integration cucumber tests

## Prerequisites

- Install `Node.js`

- Open terminal in the `tari-project\integration_tests` folder and run `npm install`

## Procedure to run

- Open terminal in the `tari-project\integration_tests` folder

- To run all tests:
  
  ```
  npm install
  ./node_modules/.bin/cucumber-js 
  ```
  or
  ```
  npm install
  node_modules\.bin\cucumber-js
  ```

- To run a specific test, add `--name <REGEXP>` to the command line.    

- To run specific tests, e.g. `critical`, add `--tags @<EXPRESSION>` to the command line.

- See `--help` for more options.

## Notes

- In Windows, running the Tari executables in debug mode fails with a stack overflow, thus Windows users must 
  run in release mode. See command line options in `baseNodeProcess.js`, `mergeMiningProxyProcess.js` 
  and `walletProcess.js`.
