#!/usr/bin/env bash
#
# Build the Minotari Ledger application and run the scenario suite against it in a Speculos simulator.
#
# That suite is the key derivation vector table plus the scenario library in `comms_testing/src/scenarios`:
# cryptographic verification of every signature the device returns, the malformed-APDU probes, the script offset
# context and ephemeral nonce store behaviour that only exists between exchanges, and the legacy nonce branch
# whitelist. `comms_testing/examples/ledger_demo.rs` runs the same scenarios against real hardware.
#
# NOTHING IN CI RUNS THIS YET. No workflow references this script or the crate it tests, so none of it has ever
# been checked by anything except a developer running the command below by hand. Wiring it up is Spec 5's job;
# until that lands, a green pull request says nothing whatsoever about the device.
#
# This script is intended to be the *only* definition of how Speculos is started, so that when CI does arrive it
# calls these subcommands rather than repeating the `docker run` in a workflow file - which is how "it passes
# locally" and "it passes in CI" stop meaning the same thing without anyone noticing.
#
# Usage:
#   ./scripts/ledger_speculos.sh build [model...]       Build the device .elf for each model
#   ./scripts/ledger_speculos.sh up [model] [seed]      Start one simulator and leave it running
#   ./scripts/ledger_speculos.sh address [model] [seed] Print a running simulator's APDU host address
#   ./scripts/ledger_speculos.sh api-address [m] [seed] Print a running simulator's HTTP API host address
#   ./scripts/ledger_speculos.sh logs [model] [seed]    Print a running simulator's log
#   ./scripts/ledger_speculos.sh down [--all]           Remove this run's simulators (--all: every run's)
#   ./scripts/ledger_speculos.sh test [model...]        Build, run the suite over every model x seed, tear down
#
# Environment:
#   MODELS                Models to cover.            Default: "nanosplus stax"
#   SEEDS                 Seeds to cover.             Default: "default alternate"
#   SPECULOS_IMAGE        Simulator image.            Default: ghcr.io/ledgerhq/speculos, digest pinned below
#   BUILDER_IMAGE         Device app builder image.   Default: ledger-app-builder :5.3.10, digest pinned below
#   SPECULOS_RUN_ID       Isolates concurrent runs.   Default: the GitHub run/job, else this shell's PID
#   JUNIT_DIR             Where JUnit XML is written. Default: <repo>/target/speculos-junit
#   SPECULOS_LOG_DIR      Where logs are written.     Default: <repo>/target/speculos-logs
#
# Host ports are **not** configurable, and do not need to be: Docker picks a free ephemeral port for each
# simulator and this script asks it which one with `docker port`. That is part of what makes two copies of this
# script safe to run at once, and it is why there is no "port 5000 is taken by AirPlay on macOS" workaround here.
#
# Concurrency, precisely: two `test` runs **on one Docker host but in separate working trees** - the shared CI
# runner case - are safe. Container names are scoped by run id and host ports are allocated by Docker, so neither
# run can see or delete the other's simulators. Two `test` runs in the *same* working tree are not supported and
# cannot be made so here: they would share the Cargo target directory, the `.elf` output directory and JUNIT_DIR,
# none of which this script owns. Give the second one its own checkout, or its own CARGO_TARGET_DIR and JUNIT_DIR.
#
# An interactive `up` simulator is in a different scope again, so a `test` run will not tear it down - *unless*
# SPECULOS_RUN_ID is set, which deliberately collapses the two scopes into one so that an operator who wants
# everything under one name gets exactly that. Set it and `test` will clean up your `up` simulators too.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WALLET_DIR="${REPO_ROOT}/applications/minotari_ledger_wallet/wallet"
COMMS_TESTING_DIR="${REPO_ROOT}/applications/minotari_ledger_wallet/comms_testing"
COMMS_TESTING_MANIFEST="${COMMS_TESTING_DIR}/Cargo.toml"

MODELS="${MODELS:-nanosplus stax}"
SEEDS="${SEEDS:-default alternate}"
# Both images are pinned by digest, not by tag.
#
# The emulator *is* the device under test. Everything this harness asserts - that nanosplus and stax agree, that
# the oracle agrees with both, that swapping the seed changes every vector - is a statement about whatever image
# was running, so a mutable tag makes a green or a red unattributable: nobody can tell later which emulator
# produced it. A tag is also a standing code-execution grant to whoever can repoint it, on every developer machine
# and every fresh CI runner, in a container that until recently ran unconfined.
#
# `:latest` was the obvious offender but `:5.3.10` is mutable too - a version tag is still just a tag - so both are
# pinned. The tag is kept in the reference for readability; when both are present Docker resolves the digest and
# ignores the tag, so the comment cannot drift into a lie.
#
# To refresh:
#   docker pull ghcr.io/ledgerhq/speculos:latest
#   docker images --digests | grep speculos          # copy the sha256: value
# and the same for the builder. Re-run `./scripts/ledger_speculos.sh test` before committing a new digest: it is a
# change of device under test, and the vector table is exactly what should have an opinion about that.
#
# Both digests are multi-arch manifest lists (linux/amd64 + linux/arm64), so they resolve on CI and on an Apple
# silicon laptop alike.
SPECULOS_IMAGE="${SPECULOS_IMAGE:-ghcr.io/ledgerhq/speculos:latest@sha256:6ed9eefd51cddd862b746719af4cd7a3265fe43d0588c388359753cab8d46d11}"
# Keep the tag in step with the DOCKER_IMAGE of the `ledger-build-tests` job in .github/workflows/ci.yml. The .elf
# this script tests is the same artefact that job builds; see "Which .elf" below.
BUILDER_IMAGE="${BUILDER_IMAGE:-ghcr.io/ledgerhq/ledger-app-builder/ledger-app-builder:5.3.10@sha256:3853136d5bba5bff4e3d5fc9d6629389e3e0c56a679b2eb4e05c97a7017bf566}"
# The one test `cmd_test` skips, on every model. Named once so the exclusion and the comment that explains it
# cannot drift apart; see `cmd_test`.
BLOCKING_PROBE="a_wrong_length_payload_does_not_block_on_a_button_press"
JUNIT_DIR="${JUNIT_DIR:-${REPO_ROOT}/target/speculos-junit}"
SPECULOS_LOG_DIR="${SPECULOS_LOG_DIR:-${REPO_ROOT}/target/speculos-logs}"

# Containers are selected by label, never by matching their names.
#
# `docker ps --filter name=...` is a regex match, so "scopes are disjoint" would have meant "disjoint as regexes",
# which is a stronger and much easier thing to get wrong than disjoint as strings - and the CI run id is built by
# concatenating `GITHUB_RUN_ID`, `GITHUB_JOB` and the attempt with hyphens, which readily produces ids that prefix
# one another. `--filter label=k=v` is an exact string comparison, so scope isolation no longer depends on the
# shape of a run id at all.
MANAGED_LABEL="tari-speculos-managed"
SCOPE_LABEL="tari-speculos-scope"

# The ports Speculos listens on *inside* its container. These are fixed because nothing else shares the container's
# network namespace; the host side is allocated by Docker, see `host_address`.
CONTAINER_APDU_PORT=9999
CONTAINER_API_PORT=5000

# Container naming, and the one thing it has to get right: **two copies of this script must not destroy each
# other's simulators.**
#
# `cmd_up` removes any container of the same name before starting, and the automatic teardown removes every
# container in its own scope. If the scope were just "tari-speculos", a second copy of this script on the same host
# - a shared or self-hosted CI runner, or a developer with `up` running in one shell while another runs `test` -
# would silently delete the first one's simulator. The victim would not see a port conflict, because the container
# it was talking to is *gone*; it would see the connection reset partway through the vector table, which reads as a
# flaky crypto test. So the scope carries a run id.
#
# The default has to be stable across the steps of one CI job (so that a later `if: always()` teardown step finds
# what the test step started) while differing between concurrent jobs. GitHub's run id, job and attempt give
# exactly that; a bare PID gives it locally, where each `test` invocation is its own run.
if [ -n "${SPECULOS_RUN_ID:-}" ]; then
  RUN_ID="${SPECULOS_RUN_ID}"
elif [ -n "${GITHUB_RUN_ID:-}" ]; then
  RUN_ID="gh${GITHUB_RUN_ID}-${GITHUB_JOB:-job}-${GITHUB_RUN_ATTEMPT:-1}"
else
  RUN_ID="$$"
fi
CONTAINER_PREFIX="tari-speculos"
# Docker names allow [a-zA-Z0-9][a-zA-Z0-9_.-]*, but `.` is deliberately **not** in the keep set here.
#
# Container selection used to be `docker ps --filter "name=^${scope}-"`, and that filter is a *regular expression*,
# not a literal prefix. A run id of `..` would have produced `^tari-speculos-..-`, which matches
# `tari-speculos-up-nanosplus-default` - the interactive scope the run id exists to protect. Selection is now by
# label (see `--label` at `docker run`), so this is belt and braces rather than the only defence, but a scope name
# that cannot contain a regex metacharacter is worth having anyway.
RUN_SCOPE="${CONTAINER_PREFIX}-$(printf '%s' "${RUN_ID}" | tr -c 'a-zA-Z0-9_-' '-')"

# `up` needs the opposite property from `test`.
#
# A `test` run owns its simulators from start to finish inside one process, so a per-invocation run id is exactly
# right. `up` deliberately outlives its invocation - the whole point is to leave a simulator for you to iterate
# against - and the `address`, `logs` and `down` that follow are *separate* invocations with, locally, a different
# PID. Scoping `up` by run id would mean `address` could never find what `up` had just started.
#
# So interactive simulators live in their own fixed scope. A concurrent `test` still cannot touch them, because
# `test` only ever removes containers under its own run scope - which is the property the run id exists to provide.
# Setting SPECULOS_RUN_ID explicitly pins both, for anyone who wants two isolated interactive simulators at once.
if [ -n "${SPECULOS_RUN_ID:-}" ]; then
  INTERACTIVE_SCOPE="${RUN_SCOPE}"
else
  INTERACTIVE_SCOPE="${CONTAINER_PREFIX}-up"
fi

# Which scope the subcommand in flight is working in. `cmd_test` switches it to the run scope; everything else
# operates on the interactive one.
SCOPE="${INTERACTIVE_SCOPE}"

# Speculos' own name for each model, which is not the name `cargo ledger build` uses. This doubles as the
# validation for a model argument: every path that takes one calls it first, so a typo fails here with a usable
# message rather than further in, as a bare arithmetic or docker error.
speculos_model() {
  case "$1" in
    nanosplus) echo "nanosp" ;;
    stax) echo "stax" ;;
    nanox) echo "nanox" ;;
    flex) echo "flex" ;;
    *)
      echo "Unknown model '$1'. Known models: nanosplus, stax, nanox, flex." >&2
      return 1
      ;;
  esac
}

# The published BIP-39 mnemonics from applications/minotari_ledger_wallet/comms_testing/src/seeds.rs. These are
# public test seeds: anything derived from them is spendable by anyone. Never restore either onto real hardware.
#
# `default` is deliberately empty: it means "pass Speculos no --seed at all", so that the default column of the
# vector table is pinned to whatever Speculos itself publishes as its default rather than to a copy of it here that
# could silently fall out of step.
seed_argument() {
  case "$1" in
    default) : ;;
    alternate)
      echo "--seed"
      echo "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art"
      ;;
    *) echo "Unknown seed '$1', expected 'default' or 'alternate'" >&2; return 1 ;;
  esac
}

container_name() { echo "${SCOPE}-$1-$2"; }

elf_path() { echo "${WALLET_DIR}/target/$1/release/minotari_ledger_wallet"; }

# Check a model/seed pair before anything is derived from it.
validate() {
  speculos_model "$1" >/dev/null
  seed_argument "$2" >/dev/null
}

# The host side of a container port, as Docker allocated it.
host_address() {
  local name="$1" container_port="$2" mapped
  mapped="$(docker port "${name}" "${container_port}/tcp" 2>/dev/null | head -n 1 || true)"
  if [ -z "${mapped}" ]; then
    echo "No host port is published for ${container_port} on ${name}; is it running?" >&2
    return 1
  fi
  echo "${mapped}"
}

# Which .elf: the one `cargo ledger build <model> -- --locked` produces, byte for byte the command the
# `ledger-build-tests` job in .github/workflows/ci.yml already runs. There is deliberately no artefact handoff
# between a build job and a test job - see Spec 2, which says to split them only once that job exceeds 15 minutes.
# Until then a split buys a slower pipeline and an upload/download step for nothing.
cmd_build() {
  local models="${*:-${MODELS}}" model
  for model in ${models}; do
    speculos_model "${model}" >/dev/null
  done
  for model in ${models}; do
    echo "==> Building the device application for ${model}"
    # On the mount, and what actually mitigates it.
    #
    # `-v "${REPO_ROOT}:/app"` hands a third-party image the working tree read-write, `.git/hooks/` included, which
    # is a path to host code execution at the next git operation.
    #
    # **The flags below do not close that.** `--user`, `--cap-drop ALL` and `no-new-privileges` constrain privilege
    # *inside* the container; none of them stops a compromised image writing anything the invoking user can write
    # *through* the mount. `--user` only decides who owns the files it drops. They are kept because they are worth
    # having on their own, but do not read them as the answer to this.
    #
    # What closes it is the digest pin on BUILDER_IMAGE. A mutable tag is a standing grant: whoever can repoint it
    # gets to run code here, on every machine, at any time, with no change to this repository. A digest cannot
    # change under you, so the grant becomes a one-off decision about one specific image that a human reviewed -
    # which is the difference between a supply-chain exposure and a supply-chain choice.
    #
    # The residual risk is the mount itself, and it is deliberately not narrowed. The correct narrowing is
    # `applications/minotari_ledger_wallet` - note *not* `.../wallet`, which would break the build, because the
    # wallet crate depends on `../common` by path. But `.github/workflows/ci.yml` mounts `${GITHUB_WORKSPACE}:/app`,
    # so narrowing only here would mean the local and CI builds stopped being the same command. Narrow it there and
    # here in one change, or not at all.
    #
    # For whoever does that: `ci.yml`'s ledger-build-tests job currently runs this builder **as root** and refers to
    # the image **by tag only**. This script is therefore strictly more hardened than CI right now - parity is
    # broken, but in the safe direction. Pinning the digest and narrowing the mount there belong in the same change.
    #
    # CARGO_HOME moves because the image's default is under /opt and is not writable by a non-root uid. It points
    # into `target/`, which is gitignored, so the registry cache survives between runs instead of being
    # re-downloaded each time.
    docker run --rm \
      --user "$(id -u):$(id -g)" \
      --cap-drop ALL \
      --security-opt no-new-privileges \
      --memory 4g \
      --pids-limit 512 \
      -e CARGO_HOME=/app/applications/minotari_ledger_wallet/wallet/target/cargo-home \
      -v "${REPO_ROOT}:/app" \
      -w /app/applications/minotari_ledger_wallet/wallet \
      "${BUILDER_IMAGE}" \
      cargo ledger build "${model}" -- --locked
  done
}

# Save each simulator's log before tearing it down, so a CI failure has something to upload. `docker logs` on a
# removed container returns nothing, so this has to happen while the container still exists.
save_log() {
  local model="$1" seed="$2" name
  name="$(container_name "${model}" "${seed}")"
  mkdir -p "${SPECULOS_LOG_DIR}"
  docker logs "${name}" >"${SPECULOS_LOG_DIR}/${model}-${seed}.log" 2>&1 || true
}

# Report a simulator that never came up: keep its log where an `if: failure()` upload can find it, and echo it so
# that a local run does not have to go looking. The log is read back from the file rather than asked of Docker a
# second time, so what you see is exactly what gets uploaded.
start_up_failed() {
  local model="$1" seed="$2" reason="$3" name
  name="$(container_name "${model}" "${seed}")"
  echo "==> ${name} ${reason}. Its log:" >&2
  save_log "${model}" "${seed}"
  cat "${SPECULOS_LOG_DIR}/${model}-${seed}.log" >&2 || true
  docker rm -f "${name}" >/dev/null 2>&1 || true
}

# Bring one simulator up and wait until it is actually serving.
#
# `docker run -d` returns as soon as the container is created, which is long before Speculos has parsed the .elf,
# loaded the right CXLIB for the API level and bound its sockets. Connecting at that point fails, or worse succeeds
# against a half-initialised device. So: start detached, then poll the HTTP API until it answers. GitHub Actions
# `services:` cannot do this at all - service containers start before any step in the job runs, so they can never
# load an .elf that a later step builds.
cmd_up() {
  local model="${1:-nanosplus}" seed="${2:-default}"
  local name elf speculos api apdu
  validate "${model}" "${seed}"
  name="$(container_name "${model}" "${seed}")"
  elf="$(elf_path "${model}")"
  speculos="$(speculos_model "${model}")"

  if [ ! -f "${elf}" ]; then
    echo "No device application at ${elf}" >&2
    echo "Build it first:  $0 build ${model}" >&2
    return 1
  fi

  # Only ever this run's own container of this name - the scope carries a run id, so this cannot reach a concurrent
  # copy of the script. What it does clear is a leftover from an earlier invocation with the same SPECULOS_RUN_ID,
  # which is the normal case when a CI job re-runs a step.
  docker rm -f "${name}" >/dev/null 2>&1 || true

  # `seed_argument` prints nothing for the default seed, so this array is legitimately empty half the time. Under
  # `set -u`, bash 3.2 - which is what macOS ships - treats plain "${array[@]}" on an empty array as an unbound
  # variable and aborts, hence the `${a[@]+...}` guard on the expansion below.
  local -a seed_args=()
  while IFS= read -r line; do
    if [ -n "${line}" ]; then seed_args+=("${line}"); fi
  done < <(seed_argument "${seed}")

  echo "==> Starting ${model} with the ${seed} seed as ${name}"
  # `-p 127.0.0.1::<port>` asks Docker for a free ephemeral host port. Letting Docker allocate is what makes
  # concurrent runs safe: two simulators can never pick the same host port, because neither of them picks.
  # The .elf is mounted read-only, and the container gets no capabilities, no privilege escalation, and finite
  # memory and process budgets. It runs an emulated binary built from this repository's sources with unrestricted
  # network egress, and - deliberately, so that `docker logs` still works after a failure - without `--rm`, so a
  # run whose teardown is bypassed leaves it resident. Resident with a ceiling is a very different proposition
  # from resident without one.
  docker run -d \
    --name "${name}" \
    --label "${MANAGED_LABEL}=true" \
    --label "${SCOPE_LABEL}=${SCOPE}" \
    --cap-drop ALL \
    --security-opt no-new-privileges \
    --memory 2g \
    --pids-limit 512 \
    -p "127.0.0.1::${CONTAINER_APDU_PORT}" \
    -p "127.0.0.1::${CONTAINER_API_PORT}" \
    -v "$(dirname "${elf}"):/elf:ro" \
    "${SPECULOS_IMAGE}" \
    --model "${speculos}" \
    --display headless \
    --apdu-port "${CONTAINER_APDU_PORT}" \
    --api-port "${CONTAINER_API_PORT}" \
    ${seed_args[@]+"${seed_args[@]}"} \
    "/elf/$(basename "${elf}")" >/dev/null

  if ! api="$(host_address "${name}" "${CONTAINER_API_PORT}")"; then
    start_up_failed "${model}" "${seed}" "published no API port"
    return 1
  fi

  # Readiness poll, written as an explicit loop rather than with curl's own --retry.
  #
  # curl --retry only retries what it considers transient: connection refused (with --retry-connrefused), timeouts,
  # and a handful of 4xx/5xx codes. It does **not** retry error 52, "empty reply from server" - and error 52 is
  # precisely what a starting Speculos produces. Docker's userland proxy binds and accepts the published port the
  # instant the container is created, so the connection is never *refused*; it is accepted and then dropped,
  # because nothing is listening on the other side yet. A --retry-connrefused poll therefore gives up on the first
  # attempt, which looks exactly like a simulator that failed to start.
  #
  # Each attempt still gets --max-time so that a wedged server cannot stall the whole loop.
  local attempt=0
  until curl --silent --fail --max-time 5 --output /dev/null "http://${api}/events"; do
    attempt=$((attempt + 1))
    if [ "${attempt}" -ge 60 ]; then
      start_up_failed "${model}" "${seed}" "never became ready after ${attempt} attempts"
      return 1
    fi
    # A container that has already exited is never going to become ready; fail now with its log rather than
    # spending a minute waiting for a process that is gone.
    if [ "$(docker inspect -f '{{.State.Running}}' "${name}" 2>/dev/null)" != "true" ]; then
      start_up_failed "${model}" "${seed}" "exited during start-up"
      return 1
    fi
    sleep 1
  done

  # Both halves of this guard matter. `cmd_up` is called as `if ! cmd_up ...`, which disables errexit for
  # everything inside it, so a failing `host_address` would not abort - and because the last statement here is an
  # `echo`, `cmd_up` would then **return 0 with an empty address**. A container that dies in the instant between
  # the readiness poll above and this lookup is exactly that case. The caller would go on to run the whole vector
  # table against an empty address, which resolves to Speculos' own default port.
  if ! apdu="$(host_address "${name}" "${CONTAINER_APDU_PORT}")" || [ -z "${apdu}" ]; then
    start_up_failed "${model}" "${seed}" "published no APDU port"
    return 1
  fi
  echo "==> ${name} is ready after ${attempt} attempt(s): APDU ${apdu}, API ${api}"
  # All four, because the review screen tests need all four: the instruction goes down the APDU socket and the
  # button presses go down the API one, and the model decides whether they are buttons or taps.
  echo "    SPECULOS_APDU_ADDRESS=${apdu} SPECULOS_API_ADDRESS=${api} SPECULOS_MODEL=${model} SPECULOS_SEED_ID=${seed}"
}

cmd_address() {
  local model="${1:-nanosplus}" seed="${2:-default}"
  validate "${model}" "${seed}"
  host_address "$(container_name "${model}" "${seed}")" "${CONTAINER_APDU_PORT}"
}

# The HTTP control API, which is a different socket on the same simulator from the APDU port above.
#
# The review screen tests need both: the instruction goes down the APDU socket and blocks until somebody presses a
# button, and the button press goes down this one. The port has always been published - `cmd_up` maps both - but
# until the UI tests existed nothing asked for it.
cmd_api_address() {
  local model="${1:-nanosplus}" seed="${2:-default}"
  validate "${model}" "${seed}"
  host_address "$(container_name "${model}" "${seed}")" "${CONTAINER_API_PORT}"
}

cmd_logs() {
  local model="${1:-nanosplus}" seed="${2:-default}"
  validate "${model}" "${seed}"
  docker logs "$(container_name "${model}" "${seed}")"
}

remove_container() {
  docker rm -f "$(container_name "$1" "$2")" >/dev/null 2>&1 || true
}

# Remove the simulators belonging to a scope. Idempotent and never fails, which is what makes it safe both as an
# `if: always()` CI step and as the EXIT trap below: a run that died before starting anything must not die again
# on the way out.
remove_scope() {
  local scope="$1" names
  names="$(docker ps -aq --filter "label=${SCOPE_LABEL}=${scope}" 2>/dev/null || true)"
  if [ -n "${names}" ]; then
    echo "==> Removing simulators in scope ${scope}"
    # shellcheck disable=SC2086
    docker rm -f ${names} >/dev/null 2>&1 || true
  fi
}

# Every simulator this script has ever started, regardless of scope.
remove_all_scopes() {
  local names
  names="$(docker ps -aq --filter "label=${MANAGED_LABEL}=true" 2>/dev/null || true)"
  if [ -n "${names}" ]; then
    echo "==> Removing every simulator this script started"
    # shellcheck disable=SC2086
    docker rm -f ${names} >/dev/null 2>&1 || true
  fi
}

# `down` removes the interactive simulators and this run's, which between them are everything the *caller* started.
# It deliberately leaves a concurrent run's alone - that is what the run scope is for.
#
# `down --all` is the big hammer for when a run was killed so hard its trap never fired and nobody knows its run id
# any more. It will take out a concurrent run's simulators too, which is exactly why it is not the default.
cmd_down() {
  case "${1:-}" in
    --all) remove_all_scopes ;;
    "")
      remove_scope "${INTERACTIVE_SCOPE}"
      if [ "${RUN_SCOPE}" != "${INTERACTIVE_SCOPE}" ]; then
        remove_scope "${RUN_SCOPE}"
      fi
      ;;
    *) echo "Unknown argument '$1' to down; expected --all" >&2; return 1 ;;
  esac
}

# Teardown on *any* exit, not just the normal path.
#
# Without this, Ctrl-C, a CI cancellation or timeout, or a `set -e` abort anywhere between `cmd_up` and the end of
# the loop leaves a Speculos container running and its port held. The single documented command has to tear down
# reliably, and "reliably" cannot mean "provided nothing went wrong".
#
# Armed by `cmd_test` and by nothing else. `up` exists precisely to leave a simulator running for you to iterate
# against, so an unconditional trap at file scope would tear down the thing you just asked for. Only the
# subcommand that owns a simulator's whole lifecycle gets to clean it up.
#
# The exit status is captured before the teardown and re-raised after it, so that cleanup cannot turn a failed run
# green - or a green one red. `remove_scope` swallows its own errors, so it cannot trip `set -e` in here either.
#
# Verified on bash 3.2 (what macOS ships) that this does not fire inside command substitution: bash resets caught
# traps in subshells, so `$(cmd_address ...)` below cannot trigger a teardown mid-run.
on_exit() {
  local status=$?
  trap - EXIT
  remove_scope "${RUN_SCOPE}"
  exit "${status}"
}

arm_teardown() {
  trap on_exit EXIT
  # Turn a signal into a normal exit so that the EXIT trap does the one teardown, with the conventional 128+signo
  # status rather than 0.
  trap 'exit 130' INT
  trap 'exit 143' TERM
}

# Where nextest writes its JUnit report.
#
# This is a *list* of candidates rather than one path, and the reason is worth writing down because the obvious
# tidy-up is wrong.
#
# Measured against cargo-nextest 0.9.78: its store directory is the manifest's **workspace root** `target/nextest/
# <profile>/`, and it honours neither `CARGO_TARGET_DIR` nor even nextest's own `--target-dir` when choosing it -
# both of those move the *build* output and leave the store where it was. So deriving the path from
# `cargo metadata --format-version 1 | .target_directory`, which is the natural-looking fix, produces a path
# nextest never writes to the moment anyone sets CARGO_TARGET_DIR, and the report then looks permanently missing.
#
# That is measured behaviour of one version, not a contract, and a later nextest may well start respecting
# CARGO_TARGET_DIR. So rather than betting on either, both candidates are cleared before a run and searched after
# it. Whichever nextest actually used is the one that will be there.
junit_candidates() {
  echo "${COMMS_TESTING_DIR}/target/nextest/ci/junit.xml"
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    echo "${CARGO_TARGET_DIR}/nextest/ci/junit.xml"
  fi
  if [ -n "${CARGO_BUILD_TARGET_DIR:-}" ]; then
    echo "${CARGO_BUILD_TARGET_DIR}/nextest/ci/junit.xml"
  fi
}

# Remove every candidate before a run.
#
# nextest rewrites its report on each run, but a run that dies before it gets there - a compile error, a killed
# process - leaves the previous one in place, and it would then be published under the *next* model/seed's name. A
# missing JUnit file is an obvious problem; a stale one that says "31 passed" is not.
clear_junit_reports() {
  local candidate
  while IFS= read -r candidate; do
    rm -f "${candidate}"
  done < <(junit_candidates)
}

# The report nextest just wrote, if any.
find_junit_report() {
  local candidate
  while IFS= read -r candidate; do
    if [ -f "${candidate}" ]; then
      echo "${candidate}"
      return 0
    fi
  done < <(junit_candidates)
  return 1
}

# JUnit comes from cargo-nextest, which writes it natively from its `ci` profile - see
# applications/minotari_ledger_wallet/comms_testing/.config/nextest.toml. `cargo test` has no JUnit output of its
# own and would need a second tool to convert its stdout, so nextest is a requirement rather than an optimisation.
# It is not auto-installed: a test script that installs software behind your back is a worse problem than a missing
# tool, and the fix is one documented line.
require_nextest() {
  if ! cargo nextest --version >/dev/null 2>&1; then
    # Piped through `sed` rather than interpolated by an unquoted heredoc: the command examples below contain
    # `$(...)` that must reach the reader literally, so the heredoc stays quoted and the one name that has to stay
    # in step with the filter in `cmd_test` is substituted by token instead.
    sed "s/@BLOCKING_PROBE@/${BLOCKING_PROBE}/g" >&2 <<'EOF'
cargo-nextest is not installed, and it is what produces the JUnit output.

  cargo install cargo-nextest --locked

Or, without JUnit, run the suite directly against a simulator you started yourself:

  ./scripts/ledger_speculos.sh up nanosplus default
  SPECULOS_APDU_ADDRESS=$(./scripts/ledger_speculos.sh address nanosplus default) \
  SPECULOS_API_ADDRESS=$(./scripts/ledger_speculos.sh api-address nanosplus default) \
  SPECULOS_MODEL=nanosplus SPECULOS_SEED_ID=default \
    cargo test --locked --manifest-path applications/minotari_ledger_wallet/comms_testing/Cargo.toml -- \
      --ignored --test-threads=1 --skip @BLOCKING_PROBE@

The --skip is needed on every model. That test documents an unfixed device bug and leaves the device unusable by
the tests after it; see its doc comment.
EOF
    return 1
  fi
}

# Build every model, then run the whole suite against every model x seed combination.
#
# One table and one scenario library, every model. If stax disagrees with nanosplus about a derived key that is a
# bug in the device application - a user moving their recovery phrase between Ledger models would find a different
# wallet - and it is never fixed by giving stax its own table. The same argument applies to the scenarios: they are
# written once and run against every model, which is how "the two builds behave the same" gets asserted at all.
cmd_test() {
  local models="${*:-${MODELS}}" model seed failures=0
  local junit_report apdu api
  # Everything this command starts belongs to the run scope, so a concurrent copy of the script - or an
  # interactive `up` sitting in another shell - is invisible to it and safe from it.
  SCOPE="${RUN_SCOPE}"
  require_nextest
  for model in ${models}; do
    speculos_model "${model}" >/dev/null
  done
  for seed in ${SEEDS}; do
    seed_argument "${seed}" >/dev/null
  done

  arm_teardown
  cmd_build ${models}
  mkdir -p "${JUNIT_DIR}"

  for model in ${models}; do
    for seed in ${SEEDS}; do
      echo
      echo "======================================================================"
      echo "  ${model} / ${seed} seed"
      echo "======================================================================"
      if ! cmd_up "${model}" "${seed}"; then
        failures=$((failures + 1))
        continue
      fi

      # Resolved before the run, not inlined into the environment prefix below.
      #
      # An assignment in an env prefix swallows its own failure: `FOO="$(thing)" cmd` runs `cmd` with `FOO` empty
      # if `thing` fails, and in an `if` condition nothing notices. Pointing the suite at an empty address would
      # mean `SPECULOS_APDU_ADDRESS=""`, and a run against whatever happens to be on Speculos' default port is the
      # one failure mode that can come back green without ever touching the device under test.
      if ! apdu="$(cmd_address "${model}" "${seed}")" || [ -z "${apdu}" ]; then
        echo "==> Could not find the APDU address for ${model} / ${seed}" >&2
        save_log "${model}" "${seed}"
        failures=$((failures + 1))
        remove_container "${model}" "${seed}"
        continue
      fi

      # The same argument as for the APDU address above, and it matters more here: an empty SPECULOS_API_ADDRESS
      # would leave the review tests driving Speculos' conventional port, which is the single most likely place
      # for somebody else's simulator to be listening. The crate refuses to fall back, but only if the variable
      # is set-and-empty rather than never set at all, so it must not be resolved in an env prefix.
      if ! api="$(cmd_api_address "${model}" "${seed}")" || [ -z "${api}" ]; then
        echo "==> Could not find the HTTP API address for ${model} / ${seed}" >&2
        save_log "${model}" "${seed}"
        failures=$((failures + 1))
        remove_container "${model}" "${seed}"
        continue
      fi

      clear_junit_reports

      # --run-ignored all, not ignored-only: the device tests and the oracle/table unit tests both matter, and
      # running them in one invocation keeps them in one JUnit file. `#[ignore]` is the "needs a device" gate in
      # that crate and nothing else, so running all of it is running the whole suite.
      #
      # ...with exactly one exclusion, named here rather than hidden behind a second `#[ignore]` reason so that it
      # cannot quietly grow into a list.
      #
      # `a_wrong_length_payload_does_not_block_on_a_button_press` documents a device bug the scenario suite found
      # and deliberately did not fix: the application's reaction to a wrong payload length leaves the device
      # unusable by an unattended host. It is excluded on **every** model, because both toolkits are affected -
      # differently, and both fatally for a suite. Measured against the pinned image:
      #
      #   nanosplus (BAGL)  `SingleMessage::show_and_wait()` blocks on a button press, so the reply never arrives.
      #                     The test burns the transport's full 120 second read timeout and fails, and the device
      #                     is left on the modal.
      #   stax (NBGL)       `NbglStatus::show` draws and returns, so the reply arrives and the test's own
      #                     assertions pass in about three seconds - but the status screen stays up, and nothing
      #                     puts the device back at its home screen, because `show_status_and_home_if_needed` in
      #                     wallet/src/main.rs only does that for GetOneSidedMetadataSignature. Every scenario
      #                     after it then fails in `expect_home`; measured as 3 of 3 vector scenarios failing.
      #
      # So "it passes on stax" is true of the test and false of the run, which is why this is not conditional on
      # the model. Read the test's doc comment before touching this; when the device stops wedging itself, delete
      # the exclusion and the test together.
      if SPECULOS_APDU_ADDRESS="${apdu}" \
        SPECULOS_API_ADDRESS="${api}" \
        SPECULOS_MODEL="${model}" \
        SPECULOS_SEED_ID="${seed}" \
        cargo nextest run \
        --locked \
        --manifest-path "${COMMS_TESTING_MANIFEST}" \
        --profile ci \
        --run-ignored all \
        -E "not test(=${BLOCKING_PROBE})"; then
        echo "==> ${model} / ${seed} passed"
      else
        echo "==> ${model} / ${seed} FAILED" >&2
        failures=$((failures + 1))
      fi

      # Always keep the log and the JUnit file, pass or fail. A green run's log is what you diff against when the
      # next one is red.
      save_log "${model}" "${seed}"
      if junit_report="$(find_junit_report)"; then
        cp "${junit_report}" "${JUNIT_DIR}/${model}-${seed}.xml"
      else
        # A missing report is a failure of the run, not a cosmetic gap. Producing JUnit is part of what this
        # script is for, and a green build that quietly uploaded nothing is worse than a red one: the next person
        # to look has no record that the vectors were ever checked.
        echo "==> No JUnit report for ${model} / ${seed}. Looked in:" >&2
        junit_candidates | sed 's/^/      /' >&2
        failures=$((failures + 1))
      fi
      remove_container "${model}" "${seed}"
    done
  done

  echo
  echo "==> JUnit XML: ${JUNIT_DIR}"
  echo "==> Simulator logs: ${SPECULOS_LOG_DIR}"
  if [ "${failures}" -ne 0 ]; then
    echo "==> ${failures} failure(s) across the model/seed matrix" >&2
    return 1
  fi
  echo "==> All model/seed combinations passed"
}

main() {
  local command="${1:-test}"
  shift || true
  case "${command}" in
    build) cmd_build "$@" ;;
    up) cmd_up "$@" ;;
    address) cmd_address "$@" ;;
    api-address) cmd_api_address "$@" ;;
    logs) cmd_logs "$@" ;;
    down) cmd_down "$@" ;;
    test) cmd_test "$@" ;;
    *)
      sed -n '3,42p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      return 1
      ;;
  esac
}

main "$@"
