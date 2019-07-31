# Formatting

PRs are checked for formatting using the command:
```
TOOLCHAIN_VERSION=nightly-2019-07-15
rustup component add --toolchain $TOOLCHAIN_VERSION rustfmt
cargo fmt --all -- --check
```

You can automatically format the code using the command:
```
cargo fmt --all
```

The toolchain version may change from time to time, so check in `.circlecit/config.yml` for the latest toolchain version