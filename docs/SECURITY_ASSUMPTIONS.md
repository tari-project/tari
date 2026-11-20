# Security assumptions

This file records the trust assumptions the code in this repository is built on. It exists so that people and
automated tools reviewing the code can tell a deliberate design choice from a defect.

It complements [SECURITY.md](../SECURITY.md), which describes how to report an issue and what is in scope.

## How to use this file

**If you are reviewing or scanning the code:**

- Read the entries below before you report a security finding.
- A finding that only works by breaking a listed assumption is expected behaviour, not a defect. Do not report it
  as one.
- A finding is still valid if it falls under an entry's **Not covered** line, or if you can show that the assumption
  itself is wrong or no longer matches the code. In that case cite the entry id and say why.
- If you rely on an assumption that is not listed here, say so in your report so it can be added or rejected.

**If you are changing the code:**

- A change that weakens or removes a listed assumption must update this file in the same pull request.
- A change that introduces a new trust relationship (one component accepting another's data or instructions without
  checking them) should add an entry.

## Adding or changing an entry

- Entries are added by pull request and count as decided once merged. Do not add an entry for behaviour that is still
  under discussion or has a fix in progress.
- Each entry has a stable id (`A-<AREA>-<NN>`). Ids are never reused; a withdrawn entry is moved to
  [Withdrawn entries](#withdrawn-entries) with the reason.
- Keep the fields below. **Not covered** is required: it states what the assumption does not excuse.

```
### A-<AREA>-<NN>: <one-line statement>

- **Assumption:** what is trusted, and by whom.
- **What that means:** what the trusted party can do as a result.
- **Not covered:** what the trusted party must still not be able to do.
- **Where:** the crates or modules it applies to.
- **Recorded:** date, and the pull request or issue where it was decided.
```

## Entries

### A-WALLET-01: The wallet trusts the base node it connects to for chain data

- **Assumption:** A wallet relies on its configured base node for the state of the chain: which outputs exist, which
  transactions are mined, the tip height and fee information. It does not validate the chain itself.
- **What that means:** A base node that lies can hide outputs or transactions from the wallet, report wrong
  confirmation status or balances, withhold submitted transactions, and learn which outputs the wallet asks about.
  Users who need stronger guarantees run their own base node.
- **Not covered:** The base node must not be able to obtain key material, make the wallet sign or broadcast a
  transaction the user did not ask for, crash the wallet, exhaust its memory or disk, or corrupt its database. Data
  from the base node is still untrusted input for parsing and size limits.
- **Where:** `base_layer/wallet`, `base_layer/transaction_components` (scanning and transaction submission clients),
  `applications/minotari_console_wallet`.
- **Recorded:** 2026-11-17.

### A-SIGNER-01: Running the offline signer without a prompt is the operator's choice

- **Assumption:** The offline signer can run interactively, where a person confirms each request, or
  non-interactively, where it signs without asking. Choosing the non-interactive mode is a deployment decision, and
  the operator is then responsible for controlling what is delivered to the signer.
- **What that means:** In non-interactive mode there is no per-request human approval. The signer signs every
  request that passes its own checks.
- **Not covered:** The checks the signer applies in either mode must hold, the summary shown in interactive mode must
  describe everything that is signed, and the interactive mode must not be bypassable without the operator selecting
  the non-interactive mode.
- **Where:** `applications/minotari_offline_signer`, `base_layer/transaction_components` (`offline_signing`).
- **Recorded:** 2026-11-17.

### A-SIGNER-02: The online wallet relies on the signature over a signed result

- **Assumption:** The online wallet records and broadcasts what a signed result file says (the recipients it shows in
  its history, the change output, the transaction) once the file passes `verify_signed_result`. For a version 7
  request the offline signer signs the result with the key that signed the request: the payload key the request
  names, otherwise the view key. The wallet verifies both signatures with a key it holds, and a wallet with an
  imported payload key accepts no other key. Results for version 5 and 6 requests carry no signature and are accepted
  for a request the wallet signed.
- **What that means:** The signature detects a change made in transit by anyone without that key. It does not show
  who produced the file: every holder of the view key can produce a view-key signature, and the online wallet itself
  holds its payload key. Such a party can make the wallet record a wrong recipient or change output; it cannot move
  funds, as the transaction still needs spend-key signatures. An unsigned version 5 or 6 result can also be changed
  in its `signed_transaction` by anyone, within what the checks in "Not covered" allow.
- **Not covered:** The file must not decide whether it is checked: a version 7 request is only answered by a signed
  result of the same version, and the request's version is covered by its own signature. Without the signing key it
  must not be possible to change any field of a version 7 result, or the request inside a result of any version,
  and have the wallet accept it. In every version the outputs the wallet records must be outputs of the transaction
  it broadcasts.
- **Where:** `base_layer/transaction_components` (`offline_signing`), `base_layer/wallet` (transaction service),
  `applications/minotari_offline_signer`, `applications/minotari_console_wallet`.
- **Recorded:** 2026-11-20, pull request #8349.

### A-MULTISIG-01: Multisig co-signers trust the party that builds the spending transaction

- **Assumption:** A co-signer's signature covers the message fixed in the output's script when the output was
  created. It does not cover the recipient, amount or fee of the transaction that later spends the output.
  Co-signers trust whoever assembles that transaction.
- **What that means:** Once the required number of signatures exists, the party building the transaction decides
  where the funds go. Co-signers should confirm out of band which output they are signing for.
- **Not covered:** Spending with fewer signatures than the threshold, using a signature for a different output's
  script message, or a non-member producing an accepted signature.
- **Where:** `base_layer/transaction_components` (`multisig`, `key_manager`), `base_layer/wallet`,
  `applications/minotari_console_wallet`.
- **Recorded:** 2026-11-17.

### A-NET-01: One process serves one network

- **Assumption:** The network (mainnet, nextnet, a testnet, and so on) is set once when the process starts. Consensus
  hashing and domain separation read that process-wide value.
- **What that means:** Code that hashes or verifies consensus data without passing a network explicitly uses the
  network of the running process. Using data from another network in the same process is not supported.
- **Not covered:** Data from a remote peer on a different network being accepted as valid on this one.
- **Where:** `base_layer/transaction_components` (consensus hashers), `common`, `hashing`.
- **Recorded:** 2026-11-17.

### A-GRPC-01: A gRPC client with valid credentials is trusted with the methods it can reach

- **Assumption:** The base node and wallet gRPC interfaces are administrative interfaces. Whoever can connect with
  valid credentials, to the methods the operator enabled, is acting for the operator. The operator is responsible
  for the bind address, authentication, TLS and the list of enabled methods.
- **What that means:** An authenticated wallet gRPC client can spend funds. An authenticated base node gRPC client
  can call every enabled method, including expensive ones.
- **Not covered:** Reaching a method without the configured credentials, reaching a method the operator disabled,
  and anything an unauthenticated or remote peer-to-peer party can cause through these interfaces. Read-only methods
  the operator exposes publicly must stay bounded in cost.
- **Where:** `applications/minotari_app_grpc`, `applications/minotari_node`, `applications/minotari_console_wallet`.
- **Recorded:** 2026-11-17.

### A-HTTP-01: The base node wallet query HTTP service is a public, unauthenticated service

- **Assumption:** The base node's wallet query HTTP service exists so that wallets can query a base node over the
  network. It is meant to be reachable by anyone, so it has no authentication and listens on all interfaces
  (`0.0.0.0`) unless `listen_ip` is set under `[base_node.http_wallet_query_service]`. Whether the port is reachable
  from outside the host is the operator's decision, made through `listen_ip`, firewall and router configuration.
- **What that means:** Anyone who can reach the port can read what the service returns (tip and header data, UTXO
  mined and spent status, transaction status, mempool fee statistics, burn output proofs) and can submit
  transactions through `/json_rpc`. All of this is public chain data or something any peer can already do over the
  peer-to-peer RPC interface, so unauthenticated access is not a privilege boundary. A node that is only reachable
  over Tor does not expose this port. The default `external_address` (`http://127.0.0.1:<port>`) is only the URL
  advertised to peers; it does not limit the bind address.
- **Not covered:** A caller must not be able to crash the node, exhaust its memory, CPU or disk, get a transaction
  into the mempool without full validation, or read anything that is not public chain or mempool data. Request
  bodies, query parameters and response sizes must stay bounded.
- **Where:** `applications/minotari_node` (`http`, `bootstrap.rs`, `config.rs`).
- **Recorded:** 2026-11-17, issue #7998.

## Withdrawn entries

None.
