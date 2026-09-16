# Speculos simulator fixture

A reproducible simulated Ledger device, and a table of key derivation vectors the device is asserted to agree with.

Everything here is test-harness only. This crate is its own Cargo workspace and is `exclude`d from the repository
root workspace on purpose; see the crate docs in `src/lib.rs` for why that is a security property and not a
packaging accident.

## The one command

```bash
./scripts/ledger_speculos.sh test
```

Builds the device application for every model, starts Speculos on each model with each seed, runs the suite against
it, saves the log and the JUnit XML, and tears the container down.

> **Nothing in CI runs any of this yet.** No workflow references this script or this crate, so the vector table has
> only ever been checked by someone running the command above by hand. Wiring it up is Spec 5's job; until that
> lands, a green pull request says nothing about the key derivation vectors. The script is written to be the single
> definition of how Speculos is started so that CI, when it arrives, calls these subcommands instead of repeating
> the `docker run` — but that is a design intent, not a thing that is true today.

Other subcommands:

```bash
./scripts/ledger_speculos.sh build [model...]           # just build the .elf
./scripts/ledger_speculos.sh up [model] [seed]          # start one and leave it running
./scripts/ledger_speculos.sh address [model] [seed]     # its APDU host address
./scripts/ledger_speculos.sh api-address [model] [seed] # its HTTP control API host address
./scripts/ledger_speculos.sh logs [model] [seed]        # its log
./scripts/ledger_speculos.sh down [--all]               # remove your simulators (--all: everyone's)
```

To iterate against a simulator you leave running:

```bash
./scripts/ledger_speculos.sh up nanosplus default
SPECULOS_APDU_ADDRESS=$(./scripts/ledger_speculos.sh address nanosplus default) \
SPECULOS_API_ADDRESS=$(./scripts/ledger_speculos.sh api-address nanosplus default) \
SPECULOS_MODEL=nanosplus SPECULOS_SEED_ID=default \
  cargo test --locked --manifest-path applications/minotari_ledger_wallet/comms_testing/Cargo.toml -- \
    --ignored --test-threads=1
```

The review screen tests need all four variables. `SPECULOS_APDU_ADDRESS` is where the instruction goes;
`SPECULOS_API_ADDRESS` is where the button presses go — two different sockets on the same simulator — and
`SPECULOS_MODEL` decides whether the harness presses buttons or taps a touchscreen. `--test-threads=1` is not
optional for a `cargo test` run: the scenarios share one device.

Host ports are not fixed and not configurable: Docker allocates a free ephemeral port per simulator and the script
asks it which one, which is why `address` exists and why there is no "port 5000 is taken by AirPlay on macOS"
workaround to remember.

### Running more than one at a time

Container names are scoped by a run id, and `test` only ever removes containers in its own scope. So:

* two `test` runs **on one Docker host in separate working trees** — the shared CI runner case — cannot see or
  delete each other's simulators;
* a `test` run will not tear down a simulator you started with `up`, which lives in its own fixed scope — with
  one exception: setting `SPECULOS_RUN_ID` deliberately collapses the two scopes into one, so `test` then cleans
  up your `up` simulators too;
* two `test` runs in the **same** working tree are not supported, and no container naming can fix that: they would
  share the Cargo target directory, the `.elf` output directory and `JUNIT_DIR`. Give the second one its own
  checkout, or at least its own `CARGO_TARGET_DIR` and `JUNIT_DIR`.

`SPECULOS_RUN_ID` pins the scope explicitly. It defaults to the GitHub run/job/attempt in Actions — stable across
the steps of one job, so a later `if: always()` teardown finds what the test step started — and to the shell's PID
locally.

`test` tears its simulators down through an `EXIT`/`INT`/`TERM` trap, so Ctrl-C, a CI cancellation or an aborted
run cleans up too, not just the happy path.

## Upgrading an existing checkout

Two one-time wrinkles, both invisible on a fresh CI runner and both only on a checkout that ran an earlier version
of `scripts/ledger_speculos.sh`:

* **`wallet/target/` may be root-owned.** The builder used to run as root, so anything it wrote through the bind
  mount belongs to root. It now runs as the invoking user, and the first hardened build will fail on those files.
  Remove them once: `sudo rm -rf applications/minotari_ledger_wallet/wallet/target`.
* **Simulators started before the labelling change are invisible to `down --all`.** Container selection is now by
  `tari-speculos-managed` label, which older containers do not carry. Clear any strays by name once:
  `docker rm -f $(docker ps -aq --filter name=^tari-speculos)`.

## How the device tests are gated

Every test that needs a device is `#[ignore]`d. That is the only gate; there is no "skip if the environment
variable is unset" anywhere.

* `cargo test` reports them as **ignored** — `libtest` prints that distinctly from "passed", so a local run is
  quiet without ever claiming the vectors were checked.
* `cargo test -- --ignored` and `cargo nextest run --run-ignored all` — what the script and CI use — run them for
  real. There is no skip path left: a missing simulator **fails**.

The alternative, gating on `SPECULOS_APDU_ADDRESS` being set, is a trap: a machine with no simulator would *pass*,
and an unchecked vector table would look exactly like a checked one. Silence is the one answer a vector table must
never be able to give.

`SPECULOS_APDU_ADDRESS`, `SPECULOS_API_ADDRESS`, `SPECULOS_MODEL` and `SPECULOS_SEED_ID` are configuration, not
gates.

For both, **unset means the default, but set-but-empty is a hard error**. That is not pedantry:
`SPECULOS_APDU_ADDRESS=$(./scripts/ledger_speculos.sh address ...)` with no simulator running yields an empty
string, and the default it would otherwise fall back to — `127.0.0.1:9999` — is Speculos' own conventional port,
so a fallback could run the whole vector table green against some unrelated simulator. Note that this default is
*not* where the script puts a simulator: it uses ephemeral ports, so ask `address` rather than assuming.

## Test output

JUnit XML is produced by **cargo-nextest**, from the `ci` profile in `.config/nextest.toml`. `cargo test` has no
JUnit output of its own. Install it with `cargo install cargo-nextest --locked`; the script tells you so rather
than installing it for you.

* JUnit: `target/speculos-junit/<model>-<seed>.xml`
* Speculos logs: `target/speculos-logs/<model>-<seed>.log`, saved for every run, pass or fail

## In CI — not wired up yet

There is no workflow for any of this. `grep -rn "comms_testing\|ledger_speculos" .github/` returns nothing, and
that is the current state, not an oversight in this document: the workflow is Spec 5's job. What follows is the
sketch Spec 5 needs, not a description of something that runs.

Until it lands, none of the hardening in this crate — the vector table, the oracle, the seed checks — is verified
by anything after merge.

```yaml
- name: ledger speculos vector tests
  # A wedged simulator must not sit until GitHub's six hour ceiling. The transport has its own 120s read timeout
  # and the nextest `ci` profile has a slow-timeout, but a job-level bound is the one that holds regardless of
  # which of them is in play.
  timeout-minutes: 45
  run: ./scripts/ledger_speculos.sh test

- name: tear down speculos
  if: always()
  run: ./scripts/ledger_speculos.sh down

- name: upload junit
  if: always()
  uses: actions/upload-artifact@v7
  with:
    name: junit-ledger-speculos
    path: ${{ github.workspace }}/target/speculos-junit/

- name: upload speculos logs
  if: failure()
  uses: actions/upload-artifact@v7
  with:
    name: speculos-logs
    path: ${{ github.workspace }}/target/speculos-logs/
    retention-days: 7
    if-no-files-found: warn
```

`cargo-nextest` must be installed on the runner (`cargo install cargo-nextest --locked`); the script fails with
that instruction rather than installing it itself.

### Which `.elf`

The one `cargo ledger build <model> -- --locked` produces — byte for byte the command the `ledger-build-tests` job
in `.github/workflows/ci.yml` already runs. There is deliberately **no** artifact handoff between a build job and a
test job. Split them when `ledger-build-tests` exceeds 15 minutes and not before; until then a split buys a slower
pipeline and an upload/download round trip for nothing.

Speculos is started with `docker run -d` plus a readiness poll, **not** with GitHub Actions `services:`. Service
containers start before any step in the job runs, so they can never load an `.elf` that a later step builds.

## Two seeds

* **Speculos' published default** (`speculos/main.py::DEFAULT_SEED`), so the vectors are reproducible by anyone
  from published inputs.
* **The BIP-39 all-zero-entropy 24 word mnemonic**, as a second seed.

The second seed is not redundancy. A single fixed seed cannot tell "the device derives correctly" apart from "the
device returns a constant": a table captured against one seed passes just as happily against a device whose
derivation has been replaced by a lookup table. Swapping the seed must change **every** vector, and a test asserts
exactly that.

> **Both mnemonics are public test seeds.** Anything derived from either is spendable by anyone, instantly, without
> compromising anything. Never restore either onto real hardware or into a wallet that will hold value.

## One table, every model

There is one vector table (`src/vectors.rs`) and it is asserted against every model. That is the assertion, not a
convenience: "the crypto is target independent" is otherwise an assumption nobody checks, and the `stax`/`flex`
half of the device application is a genuinely different build — 79 `target_os` sites across 11 files, including a
different main event loop.

**A `stax` failure is never fixed by forking the table.** If `stax` disagrees with `nanosplus` about a derived key,
a user who moves their recovery phrase between Ledger models finds a different wallet, and their funds are gone. A
forked table would hide precisely that.

As of this writing `nanosplus` and `stax` return byte-identical values for every row under both seeds.

---

## Step 0 spike: what the Ledger SDK puts in `bip32_derive`'s 64-byte buffer

**Question.** `wallet/src/utils.rs::get_raw_bip32_key` hands `bip32_derive` a 64-byte buffer and then hashes the
whole thing. "Private key ‖ chain code" is the obvious reading of a 64-byte BIP32 output, but that is an SDK
implementation detail, not a documented spec. Is the result reproducible from published BIP32 given the seed?

**Answer: yes — and the obvious reading is wrong.** The buffer is `bip32_private_key ‖ [0u8; 32]`. The second half
is never written by anything; it is the zeros the buffer was initialised with, and it is hashed as zeros.

Three pieces of evidence:

1. **The chain code is a separate out-parameter.** `ledger_device_sdk::ecc::bip32_derive` has signature
   `(curve, path, key: &mut [u8], cc: Option<&mut [u8]>)`. `get_raw_bip32_key` passes `cc: None`, so the chain code
   is never written into `key_buffer` at all. The SDK's own `impl SeedDerive for Secp256k1` confirms the split: it
   asks for the chain code in `cc` and takes only `tmp[..32]` as the key.

2. **The syscall writes 32 bytes for a Weierstrass curve.** Read directly out of Speculos'
   `src/bolos/os_bip32.c::hdw_bip32`, whose tail is:

   ```c
   if (private_key != NULL) { memcpy(private_key, key->private_key, 32); }
   if (chain      != NULL) { memcpy(chain,       key->chain_code,  32); }
   ```

   **Scope of that claim.** This is verified for Speculos, which is what the fixture runs. BOLOS is closed, so the
   same claim about real hardware rests on the syscall ABI being shared — Speculos emulates it — and on the SDK's
   own `SeedDerive for Secp256k1` taking the chain code from `cc` rather than from the tail of the key buffer.
   Strong, but inference rather than a reading. Nothing here has been run against a physical device.

3. **The SDK's `key.len() >= 64` requirement is a maximum across curves, not a statement about secp256k1.** The
   Ed25519-BIP32 path (`hdw_bip32_ed25519`) really does write 64 bytes, because its key is the `kL ‖ kR` pair:
   `memcpy(private_key, kP, 64)`. secp256k1 is not that case.

Everything upstream is textbook. Speculos' `expand_seed` is BIP-32 master key generation with the HMAC-SHA512 key
`"Bitcoin seed"`, and `hdw_bip32` is BIP-32 CKDpriv including the "IL ≥ n or child == 0, retry with `0x01 ‖ IR`"
rule. So the full construction, given only the seed, is:

```
BIP-39 mnemonic
  → PBKDF2-HMAC-SHA512(salt "mnemonic", 2048 rounds) → 64 byte seed
  → BIP-32 secp256k1, path m/44'/535348'/{account}'/0/{index}'/{key_type}
  → 32 byte private key, right-padded with 32 zero bytes
  → domain separated Blake2b-512, domain "com.tari.minotari_ledger_wallet", label "raw_key"
  → RistrettoSecretKey::from_uniform_bytes  (wide reduction)
```

**The oracle is buildable, and it is built** — `src/oracle.rs`, from published specifications only, with no
dependency on a BIP-32 or BIP-39 crate. It is itself checked against BIP-32's own published Test Vector 1 and
against Speculos' transcribed default seed bytes, and it reproduces all 22 device-captured values (11 rows × 2
seeds) exactly. The oracle disagreeing with the table fails the build.

### The fragility this pins down

Because the second half of the hash preimage is *uninitialised-by-contract* rather than chosen, every key this
wallet owns depends on the SDK continuing not to write there. A future `ledger_device_sdk` or BOLOS release that
decided to fill the remaining 32 bytes — with the chain code, say — would silently change **every derived key on
every account**, with no compile error anywhere, and existing users would lose access to their funds. There is
nothing in the type system or the SDK's documented contract that prevents it.

The vector table is the thing that turns that into a failing test. That, more than regression-catching, is why it
exists.

### A second thing the spike turned up: the account wraps at 2³²

`derive_from_bip32_key` renders the `u64` account and index into a decimal string and hands it to the SDK's
`make_bip32_path`, which parses each element back with `acc = acc * 10 + digit` into a **`u32`**. The device
application is a release build, so that wraps rather than panicking.

The host sends `u64` accounts — `accessor_methods.rs` fills them from `rand::rng().next_u64()` — so accounts 2³²
apart address the same key. Confirmed on the device: account `0` and account `4294967296` return an identical
public key, on both `nanosplus` and `stax`, under both seeds. `vectors::ACCOUNT_WRAP_VECTOR` and
`the_account_wraps_at_u32` pin the behaviour so that a change to it is caught here rather than by a user whose
account silently moved.


---

## The review screen

Exactly one instruction shows anything to a human: `GetOneSidedMetadataSignature` puts up a transaction review and
does not answer until somebody approves or rejects it. `src/review.rs`, `src/approver.rs` and `src/speculos_api.rs`
drive and assert that one screen, and `tests/speculos_review.rs` is the scenario file.

### The assertion is the field text

`Amount`, `Receiver`, and the presence or absence of `Payment ID`. That is the highest-value assertion in this
crate: a wrong key bricks a wallet, but a wrong *displayed* address steals funds, because the screen is the only
thing a user can trust when the host cannot be.

The comparison is an **equality**, not a containment: the toolkit's own furniture (titles, footers, page counters)
is stripped and what is left must be exactly the expected fields, name then value, in order. Containment was tried
first and is not good enough — the expected address is a prefix of an address with extra characters appended, which
is a different address. Equality also means a `Payment ID` row nobody asked for cannot survive, whether or not the
scenario remembered to assert it absent.

Screenshot baselines were considered and rejected. They fail on pixel changes that harm nobody, and the fix for a
red baseline is to re-bless it — which also blesses any real change hiding behind the cosmetic one. Both failure
modes teach reviewers to stop reading the diff.

### Two frontends, one scenario library

`Approver` has two implementations. `SpeculosApprover` presses a simulator's buttons and reads the text back out of
Speculos' event stream. `HumanApprover` prints the expected fields, asks an operator to confirm the device in their
hand shows exactly those, and waits while they press the button — **the operator is the assertion oracle**, which
is the only thing a real device's screen can be checked against.

The consequence is deliberate: a scenario that cannot say what it expects on screen cannot run on hardware. That is
what keeps one scenario library usable by both frontends.

Ragger, Ledger's Python test framework, is deliberately not used. A Python stack for one model's touchscreen would
re-split the scenario library into Rust scenarios for `nanosplus` and Python scenarios for `stax`, and they would
stop testing the same thing within a month.

### Event driven, no sleeps, no retries

Every wait blocks on Speculos' `GET /events?stream=true`, which returns when the device draws and not before. There
is no `sleep` and no retry loop anywhere in the harness. A retry is worse than useless here: the nonce store and the
script offset context are exactly the state a second attempt disturbs, so a test that goes green on a retry has
usually destroyed the evidence that it was red. **A test that needs a retry is a bug report.**

When a wait does run out, the failure carries the full event log, the last screen, and a PNG of it. A timeout with
no diagnostics is the single failure mode most likely to make people give up on a suite.

One subtlety that cost an afternoon and is worth not rediscovering: waiting for *an event* is not the same as
waiting for *a screen*. Events are queued, so at any moment there are usually several of the current page's own
draws still unread. The NBGL hold-to-sign gesture has to keep the finger down until the ring fills; a "press, wait
for an event, release" loop lifts the finger within milliseconds of a stale queued event and NBGL cancels the hold.
Everything therefore waits on a predicate over the *current screen*, re-read on each wake.

### The device is not restarted between scenarios

Each scenario starts by asserting the device is at its home screen. Restarting Speculos instead would be easier and
would delete the only state worth testing: the ephemeral nonce store (`EphemeralNonceCtx`, eight slots, alive for
the life of the application) and the script offset context (`ScriptOffsetCtx`, accumulated across chunks, reset by
any interleaved instruction). A suite that tears the device down between scenarios cannot see either.

---

## Step 0 spike: does Speculos' event stream carry widget geometry for NBGL?

**Question.** If it does, taps can be aimed by text anchor — find the widget whose text matches, tap the centre of
its rectangle — and survive an SDK that moves a button. If it does not, `stax` needs a per-model coordinate table,
annotated with the SDK version it was measured against.

**Answer: yes.** Every event Speculos reports carries `x`, `y`, `w` and `h`, on NBGL and on BAGL alike. Measured
against the pinned image, a `stax` running this application:

```console
$ curl -s "http://$API/events?currentscreenonly=true"
{"events": [{"text": "Sign transaction", "x": 76,  "y": 230, "w": 248, "h": 40, "clear": false},
            {"text": "to send",          "x": 143, "y": 270, "w": 114, "h": 40, "clear": false},
            {"text": "Reject",           "x": 43,  "y": 610, "w": 73,  "h": 31, "clear": false},
            {"text": "3 of 3",           "x": 246, "y": 610, "w": 67,  "h": 31, "clear": false},
            {"text": "Hold to sign",     "x": 24,  "y": 496, "w": 183, "h": 40, "clear": false}]}
```

So every button press in this harness — `Hold to sign`, `Reject`, `Yes, reject` — is found by its text and tapped
at the centre of that rectangle. There is **no** widget coordinate table.

There is one coordinate constant, and it is not a widget: the page **swipe** is a movement across the whole screen
rather than a press on a widget, so it needs the screen's dimensions. `approver::nbgl_swipe` carries them, annotated
with `ledger_device_sdk = "=1.35.0"` and `ledger_secure_sdk_sys = "=1.16.3"`.

### The spike's uninvited finding: BAGL text is lossy

NBGL hands Speculos the string it drew. **BAGL does not.** A BAGL device draws one glyph bitmap at a time, and
Speculos reconstructs the text by matching each bitmap against its own built-in copy of the font tables
(`speculos/mcu/ocr.py`). Where the application's font table and Speculos' copy disagree, the reconstruction is
wrong or absent — *while the pixels on screen are perfectly correct*.

Comparing all 96 of `ledger_device_sdk 1.35.0`'s `OPEN_SANS` glyph bitmaps against Speculos'
`find_char_from_bitmap` gives the complete disagreement, which is two characters:

| Glyph | Reported as | Why |
|---|---|---|
| `S` (regular face) | *nothing at all* | SDK bitmap `0027080606410e0000`, Speculos' nearest `8027080606411e0000`; no font matches, so the glyph is dropped |
| `I` (both faces) | `l` | a near-miss match against lower case L |

`review::as_bagl_reports` applies exactly those two substitutions to the expected text before comparing, and
nothing else — a general "normalise the string" step would absorb real differences too. The two cost very
different things:

* **`I` costs nothing, because of the base58 alphabet.** Both encoders involved — `TariAddress::to_base58` and the
  device's `tari_dual_address_display` — use `bs58`'s Bitcoin alphabet, which omits `0`, `O`, `I` and `l` precisely
  because they are easy to confuse by eye. No address and no `"{n} bytes"` can contain either side of the
  conflation. The only `I` in the whole review is in the literal label `Payment ID`, which reaches the event stream
  as `Payment lD` on `nanosplus` — harmless when both sides are conflated, and silently never-matching for anyone
  who compares the raw label instead. That is a property of the encoding, not luck.
* **`S` does cost something.** It *is* in the base58 alphabet, so on BAGL models a device that dropped an `S` from
  the address it displayed would not be caught — a real, narrow hole. Every other character is compared exactly,
  and `stax` reports text losslessly and compares all of them, which is one more reason the suite runs on both
  models rather than one.

To re-derive this after an SDK or image bump: dump `OPEN_SANS_REGULAR_11PX_CHARS` and
`OPEN_SANS_EXTRABOLD_11PX_CHARS` from the SDK's `src/ui/fonts/opensans.rs`, and feed each bitmap to
`speculos.mcu.ocr.OCR.find_char_from_bitmap` inside the Speculos container.

---

## Two bugs the spike found, fixed here

Neither is in the harness. Both were found by trying to assert things nobody had asserted before.

**`UserCancelled` never reached the host.** `ledger_device_sdk`'s `StatusWords::UserCancelled` is `0x6985`;
`minotari_ledger_wallet_common`'s `AppSW::UserCancelled` said `0x6e04`. The device replies with the SDK's value —
`wallet/src/main.rs` defines that one variant from `StatusWords` rather than from `common` — so
`ledger_get_one_sided_metadata_signature`'s `retcode() == AppSW::UserCancelled` test could never be true, and a user
who pressed Reject got `Processing("insufficient data - expected 161 got 0 bytes")` instead of
`LedgerDeviceError::UserCancelled`. The constant is corrected, and `wallet/src/main.rs` now carries `const _:`
assertions tying both SDK-derived status words to `common`'s copy, so the next such drift is a build failure rather
than a silent one.

**A rejected review left Stax and Flex staring at the dialog.** `show_status_and_home_if_needed` returned to the
home screen for `Deny` and `Ok` but not for `UserCancelled`, so on NBGL models a rejected transaction left the
"Reject transaction?" dialog on screen indefinitely. The device was still perfectly usable — the next instruction
was served normally — which is exactly what made it hard to notice and indistinguishable on screen from a hang.
BAGL models never had the problem: their main loop redraws the home menu on every pass.
