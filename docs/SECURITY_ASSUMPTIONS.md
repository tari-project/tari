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
  request that passes its own checks. The console wallet's offline-payload signing commands and
  `sign-script-message` follow the same rule, and treat stdin that is not a terminal as non-interactive.
- **Not covered:** The checks the signer applies in either mode must hold, the summary shown in interactive mode must
  describe everything that is signed, and the interactive mode must not be bypassable without the operator selecting
  the non-interactive mode.
- **Where:** `applications/minotari_offline_signer`, `base_layer/transaction_components` (`offline_signing`),
  `applications/minotari_console_wallet` (offline-payload signing commands, `sign-script-message`).
- **Recorded:** 2026-11-17.

### A-SIGNER-02: The online wallet relies on the signature over a signed result

- **Assumption:** The online wallet records and broadcasts what a signed result file says (the recipients it shows in
  its history, the change output, the transaction) once the file passes `verify_signed_result` and the checks against
  the wallet's own records. For a version 7 request the offline signer signs the result, including its receipt, with
  the key that signed the request: the payload key the request names, otherwise the view key. The wallet verifies
  both signatures with a key it holds. A view-only wallet with an imported payload key accepts no other key; a wallet
  that holds the spend key derives every payload key and accepts a request under any of them. Results for version 5
  and 6 requests carry no signature and are accepted for a request the wallet signed.
- **What that means:** The signature detects a change made in transit by anyone without that key. It does not show
  who produced the file: every holder of the view key can produce a view-key signature, and the online wallet itself
  holds its payload key. Such a party can make the wallet record a wrong recipient or change output; it cannot move
  funds, as the transaction still needs spend-key signatures. An unsigned version 5 or 6 result can be changed by
  anyone, and a version 7 result by a holder of the key that signed its request, within what the checks in "Not
  covered" allow. In such a result:
  - its `tx_id` (to one the wallet does not use yet), its receipt, the memo stored with the change output and its
    `outputs` list are not checked;
  - the signed transaction is not validated before its inputs are locked and it is recorded. A file can keep the
    inputs, the sent and change hashes and the change output, and carry a kernel signature or offset that was
    changed. It passes every check and is recorded as completed, and the base node rejects the transaction. The
    broadcast then cancels the record and releases the inputs, so the file releases the locks of the prepared
    payload it names. The genuine result for that request is refused afterwards, as its `tx_id` is in use. Closing
    this needs the transaction validated before anything is locked;
  - the change output can be left out. A result with no change output and no change hashes passes. Its transaction
    is broadcast, but the change is not stored, and the balance stays too low until the wallet scans for its outputs
    again;
  - which output is recorded as sent can be changed. With several recipients the sent hashes can be reordered. With
    one recipient and no listed change, the change output can be recorded as the sent output.

  The request's expiry limits when the signer may sign, and the signer enforces it. The wallet does not refuse a
  result because it comes back after the request has expired: while the request's inputs are locked under its
  `tx_id`, the result is accepted, and refusing it would leave those inputs locked. Releasing a prepared payload's
  locks does not revoke a copy that was already signed: the result of a cancelled payload is still accepted while
  its inputs are `Unspent` and it has not expired (a version 5 payload never expires), and locks them again.
- **Not covered:** The file must not decide whether it is checked: a version 7 request is only answered by a signed
  result of the same version, and the request's version is covered by its own signature. Without the signing key it
  must not be possible to change any field of a version 7 result, its receipt included, or the request inside a
  result of any version, and have the wallet accept it. In every version, and before the wallet writes anything:
  - the transaction must spend exactly the request's inputs, each of them once;
  - there must be one distinct sent output per recipient, no output may be both sent and change, and every recorded
    hash must be an output of the transaction;
  - a result that lists change must carry the change output, and the change output must match its own contents: its
    hash recomputed from its fields, its commitment opening to its value under its mask key, its script paying to
    its script key, and its `script_lock_height` and `input_data`, which the hash does not cover, as the signer
    writes them (`0` and empty). No change output is stored whose commitment does not open to its stated value;
  - no record is created under a `tx_id` the wallet already has a record or outputs for;
  - if outputs are locked under the request's `tx_id`, the signed inputs must be exactly those outputs and all of
    them still locked; if none are, the request must not have expired (version 6 or later) and every input must be
    `Unspent`.

  A file that fails a check locks nothing, and releases nothing except in one case: the request is one the wallet
  signed and one of its own inputs is gone, and then only the outputs locked under that request's `tx_id` are
  released.
- **Where:** `base_layer/transaction_components` (`offline_signing`), `base_layer/wallet` (`transaction_service`,
  `output_manager_service`), `applications/minotari_offline_signer`, `applications/minotari_console_wallet`.
- **Recorded:** 2026-11-25.

### A-MULTISIG-01: Multisig co-signers trust the party that builds the spending transaction

- **Assumption:** A co-signer's signature covers the message fixed in the output's script when the output was
  created. It does not cover the recipient, amount or fee of the transaction that later spends the output.
  Co-signers trust whoever assembles that transaction.
- **What that means:** Once the required number of signatures exists, the party building the transaction decides
  where the funds go. A signature is valid for every output whose script has the same message and key, so
  co-signers should take the message from an output they know independently, not only from the party asking.
  With `--output-hash`, the console wallet's `sign-script-message` refuses unless its base node has that output
  unspent with this message and key; that does not tie the signature to it.
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
- **What that means:** An authenticated wallet gRPC client can spend funds, and can release the inputs locked by a
  prepared offline-signing payload with `CancelTransaction`. An authenticated base node gRPC client
  can call every enabled method, including expensive ones.
- **Not covered:** Reaching a method without the configured credentials, reaching a method the operator disabled,
  and anything an unauthenticated or remote peer-to-peer party can cause through these interfaces. Read-only methods
  the operator exposes publicly must stay bounded in cost.
- **Where:** `applications/minotari_app_grpc`, `applications/minotari_node`, `applications/minotari_console_wallet`.
- **Recorded:** 2026-11-17.

### A-GRPC-02: The miner, merge-mining proxy and node MCP server trust the base node they connect to

- **Assumption:** The base node a miner, merge-mining proxy or node MCP server connects to over gRPC is the operator's
  own, and so is the network path to it unless TLS is configured. These clients do not re-verify the block templates
  and blocks the node returns, including the coinbase and the merge-mining hash.
- **What that means:** A node the operator does not control, or anyone on a plaintext network path, can redirect
  mining rewards and read Basic credentials. Plaintext to a non-loopback host is allowed and logs a warning on each
  connect; it is never refused. Only `http://`, `https://` and `unix:` socket addresses are accepted. An `https://`
  address with no CA configured is verified against whichever tonic root features are compiled into the binary: the
  system store always, plus the bundled Mozilla (webpki) roots where `tls-webpki-roots` is enabled (the merge-mining
  proxy, and possibly other binaries in a workspace-wide build through Cargo feature unification). Any of those
  authorities can vouch for the node.
- **Not covered:** Sending credentials to a party they were not configured for: the merge-mining proxy's p2pool node
  receives only the `p2pool_grpc_*` credentials and TLS settings, never the base node's. The miner's p2pool
  connection gets the base node's credentials and TLS settings only when it reuses `base_node_grpc_address`; a
  prompted or default p2pool address gets none. Connecting in plaintext when
  TLS is configured. Unbounded memory from node responses (messages are capped at 8 MiB by default). Blocks returned
  by a p2pool node, which are still checked for the configured coinbase address.
- **Where:** `clients/rust/base_node_grpc_client`, `applications/minotari_miner`,
  `applications/minotari_merge_mining_proxy`, `applications/minotari_mcp_node`, `applications/minotari_app_utilities`.
- **Recorded:** 2026-11-25, in the pull request that added the base node gRPC client constructor.

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

### A-MCP-01: The MCP caller acts for the operator within the tools the server exposes

- **Assumption:** The client of the base node and wallet MCP servers (normally an AI agent) is acting for the
  operator, within the tools the server exposes. Read-only tools are always exposed. Control tools (submitting
  blocks and transactions, transfers, atomic swaps, coin splits, claims and refunds) are exposed only when the
  operator starts the server with control operations enabled (`control_enabled`). The operator is responsible for
  who can talk to the server's stdio and for deciding whether to enable control operations.
- **What that means:** With control operations enabled, the agent can submit blocks and transactions through the
  node, and can move wallet funds, without a per-call human confirmation (the wallet server refuses to start with
  `require_confirmation` and control operations both set). The agent may be prompt-injected, including by text
  that reaches it through tool results. Text that comes from upstream (gRPC error messages, peer user agents, ban
  reasons, addresses and protocols, wallet failure messages) is cleaned of control and invisible format characters,
  capped in length, and quoted or labelled as untrusted (`upstream: "..."`, `untrusted_peer_claims`); it is not
  neutralised and can still carry instructions. On the wallet side, the damage is limited by a cumulative spend
  budget per server process (default 1,000,000 Tari) and a `fee_per_gram` ceiling (default 10,000 µT/g) on every
  fund-moving call, both configurable. Transfers, atomic swaps and coin splits are charged their amount plus an
  upper-bound estimate of their fee; claims and refunds are charged their fee estimate. A call that fails after funds
  may have left the wallet is still charged, as is a call the wallet accepts but reports as failed, so failed or
  unconfirmed attempts can also use up the budget. The budget is held in memory and a server restart resets it. It
  only limits calls made through the MCP server: an agent or process that can reach the wallet's gRPC interface
  directly (the MCP server does not yet authenticate to it, so it must be left open to local clients; see A-GRPC-01),
  or that can restart the MCP server, can bypass or reset it. The node MCP server, unlike the wallet one, can
  authenticate to its base node (Basic credentials from the environment or a password file, and TLS); see A-GRPC-02.
- **Not covered:** Reaching a control tool when control operations are disabled, calling a gRPC method the server does
  not list as a tool, passing a discovered tool arguments its input schema does not accept, moving more than the
  configured spend budget or paying more than the fee ceiling through the MCP server, and making the server use
  unbounded memory through upstream data (stream results and peer text must stay bounded). A single request line from
  the client itself is not yet bounded by the stdio transport.
- **Where:** `applications/minotari_mcp_common`, `applications/minotari_mcp_node`, `applications/minotari_mcp_wallet`.
- **Recorded:** 2026-11-23.

### A-MMPROXY-01: The merge-mining proxy treats upstream monerod nodes as untrusted, and its listener as trusted miners only

- **Assumption:** The `monerod_url` entries, including the public defaults (many of them plain `http://`), are
  untrusted third parties. TLS to monerod deliberately skips certificate checks (PR #7060) so the proxy can fall back
  to raw IP addresses. The Tari coinbase and reward come over gRPC from the operator-configured base node, or from the
  p2pool node when `p2pool_enabled` is set (the proxy checks that its coinbase pays the configured wallet address),
  never from monerod. The listener (loopback by default) serves trusted miners; exposing it is the operator's choice.
- **What that means:** A monerod node can serve bad or stale templates or withhold responses, which costs
  merge-mining availability. The proxy moves on to another configured monerod when a request times out or fails, or
  when the response body is too large or not valid JSON (including empty or HTML error pages); a monerod that
  returns an error status with a JSON body, or a template the proxy rejects, stays selected. The client that
  forwards requests to monerod does not follow redirects, so monerod cannot point it at other hosts. Monerod cannot
  change the Tari coinbase or reward destination, but with `submit_to_origin` on (the default) a found block goes to
  monerod first, so a selected monerod can stop found Tari blocks from being submitted; running your own monerod
  avoids this, and setting `submit_to_origin = false` keeps the block from being sent to monerod (submission still
  waits for, and can fail on, monerod selection). The Monero side is exposed: monerod builds the Monero block
  template, so an untrusted or on-path monerod can pay the Monero block reward to someone else, and it sees
  `monerod_username`/`monerod_password` when `monerod_use_auth` is set (in cleartext over `http://`, and over
  `https://` too because certificates are not checked). Operators who care should run their own monerod (see the
  SECURITY note in `common/config/presets/f_merge_mining_proxy.toml`). Requests the proxy does not recognise are
  forwarded to monerod, which should run with `--restricted-rpc`.
- **Not covered:** A monerod response must not be able to crash the proxy. Each response body is read under a fixed
  raw-byte limit (`MAX_MONEROD_RESPONSE_BYTES`, 8 MiB) and the per-request monerod timeout; the parsed JSON tree for
  a body within the limit can be many times larger per in-flight request, which is accepted. Upstream data must not
  change the Tari coinbase or the reward destination.
- **Where:** `applications/minotari_merge_mining_proxy` (`proxy/inner.rs`, `proxy/utils.rs`, `run_merge_miner.rs`,
  `config.rs`).
- **Recorded:** 2026-11-25, PR #8434.

### A-BUILDER-01: Callers keep the key ids of builder-made recipient outputs to themselves

- **Assumption:** `TransactionBuilder` treats the sender offset key it reserves for a recipient spec as one the view
  key holder cannot learn, and counts it toward the rule that a transaction spending a spend-key-derived input keeps
  at least one sender offset key with the signer. That key's id (wrapped under the view key) still reaches the
  returned `FinalizedTransaction`: in every `RecipientKeys` mode it is each `sent_outputs` entry's
  `sender_offset_key_id` (crate private; `OutputPair` has no serde), and with `RecipientKeys::DiffieHellman` it is also
  embedded in that entry's `custom_recovery_key_id` and in the output's `commitment_mask_key_id` in `sent_outputs` and
  `spec_outputs`. When no change key stays inside the builder, the calling flow is trusted not to persist or export
  those key ids.
- **What that means:** A flow that stores those key ids, or hands them to a party holding only the view key, gives that
  party every sender offset key of a transaction without change, and with the published script offset, the spend key.
  The offline signer hands its recipients' key ids back to the online wallet; that is safe only because
  `with_change_required` keeps the change key with the signer, and dropping that requirement there would be unsafe.
- **Not covered:** The builder must still refuse such a transaction when every sender offset key was handed back by
  `reserve_sender_offset_keys` or derived by the host (`with_host_derived_partial_script_offset`), whatever the caller
  does with them.
- **Where:** `base_layer/transaction_components` (`transaction_builder`, `multisig`), `base_layer/wallet`.
- **Recorded:** 2026-11-25.

## Withdrawn entries

None.
