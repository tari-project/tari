# Logging in Tari

All source Tari modules use the standard `log` crate to write log messages. This provides flexible options to the end
user (tests / applications / dynamic libraries) as to how logs are managed.

## Setup for no logging
If you're writing tests or applications on Tari and don't want to see log messages, do nothing. Log messages are
suppressed by default.

## Setup for stdout logging

This setup is usually used for tests.

If you want messages dumped to stdout without any fancy configuration, include `simple-logger` in your `Cargo.toml`
file, under `dependencies` or `dev-dependencies` as appropriate and then initialise the logger at the top of your tests
or application with `simple-logger::init_logger()`.

## Bespoke and file-based logging

This setup is usually used for applications.

`log-4rs` is a really handy crate that allows you to specify _exactly_ how and where log messages are put. The sample
configuration files in this directory provide a good start for setting up a useful logging solution.

The `log4rs-sample.yml` file defines a configuration where only error messages are written to the console, typically low
signal-to-noise comms messages are stored in one file, and general log messages are stored in another.

The `log4rs-debug-sample.rs` file has a similar setup, but logs more information useful for debugging, such as the
source code line number, and the thread that caused the log message to be emitted.

You can use these files as a starting point, or create your own.

To set up logging at the application level, we recommend the following pattern:

1. Call `log4rs::init_file(path, Default::default()).unwrap();` as soon as possible in your app, possibly as the very
   first line.
2. Obtain the path to the log configuration file. By convention, the following precedence set is recommended:
   1. from a command-line parameter, `log-configuration`,
   2. from the `TARI_LOG_CONFIGURATION` environment variable,
   3. from a default value, usually `~/.tari/log4rs.yml` (or OS equivalent).

There is a convenience function provided by this crate that will provide the path for you, see
`get_log4rs_configuration_path()`