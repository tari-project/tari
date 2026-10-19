# Fixtures

## How to update the commit proof fixture

`commit_proof.json` is a JSON-serialized `SidechainBlockCommitProof` over a test header, committed by a 3-chain of
certificates that all four members of `tests/support/mod.rs::fixture_committee` sign. Each certificate carries the
signer bitmap `0f` and its signatures in ascending order of the signers' public keys.

The `the_fixture_is_the_proof_the_fixture_committee_signs` test checks that the file matches the proof the test
support builds. After changing that proof or its encoding, regenerate the file:

```sh
cargo test -p tari_sidechain --test commit_proof -- --ignored regenerate_fixture
```
