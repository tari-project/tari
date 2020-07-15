# Point Release checklist

THings to do before pushing a new commit to `master`:

* Create new `rc` branch off development.
* Update crate version numbers
* Check that all tests pass in development (`cargo test`, `cargo test --release`)
* Publish new crates to crates.io (`./scripts/publish_crates.sh`)
  * Fix any issues with publishing
* Rebase onto master (from rc branch, `git reset --soft master` and `git commit`)
* Tag commit
* Write release notes on GitHub.
* Merge back into development (where appropriate)
* Delete branch

| Crate                        | Version | Last change                              |
|:-----------------------------|:--------|:-----------------------------------------|
| infrastructure/derive        | 0.0.10  | 7d734a2e79bfe2dd5d4ae00a2b760614d21e69c4 |
| infrastructure/shutdown      | 0.0.10  | 7d734a2e79bfe2dd5d4ae00a2b760614d21e69c4 |
| infrastructure/storage       | 0.2.0   |                                          |
| infrastructure/test_utils    | 0.2.0   |                                          |
| base_layer/core              | 0.2.0   |                                          |
| base_layer/key_manager       | 0.2.0   |                                          |
| base_layer/mmr               | 0.2.0   | d4780a588942d9bd5f3188fa373e9aa869a26870 |
| base_layer/p2p               | 0.2.0   |                                          |
| base_layer/service_framework | 0.0.10  | 7d734a2e79bfe2dd5d4ae00a2b760614d21e69c4 |
| base_layer/wallet            | 0.2.0   |                                          |
| base_layer/wallet_ffi        | 0.2.0   |                                          |
| common                       | 0.2.0   |                                          |
| comms                        | 0.2.0   |                                          |
| comms/dht                    | 0.2.0   |                                          |
| applications/tari_base_node  | 0.4.2   | 1f097732faf957323129d3bcfe073dd6dbdf8e41 |
