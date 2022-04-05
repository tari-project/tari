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

# Clippy

Currently, there isn't a neat way provided by cargo or clippy to specify
a common lint file for every project, so the lints are defined in 
`lints.toml` and you will need to install `cargo-lints` to run them.
This is done on the CI Github Actions, so you will need to pass them.
They can be run locally using:

`cargo lint clippy --all-targets`

> Note: Generally, you should not put explicit crate level `deny` attributes
> in the code, and prefer to put them in the `lints.toml`. You SHOULD however, 
> put crate level `allow` attributes, so that developers running `cargo clippy` 
> will not encounter these warnings