# Contributing Guidelines

You want to contribute to Tari, and we want you to contribute! So why is it sometimes hard to get this done?

This is financial software, so we have to take precautions to ensure that harmful code doesn't slip in anywhere.
We know that _you_ are a great person and are just trying to help. But there are not-so-nice people
out there that want to see the world burn. These guidelines are primarily for those miscreants, but everyone
needs to follow them.

These guidelines are not cast in stone, or should we say, not committed to the blockchain. We are constantly
evaluating what works, what doesn't work and will update the guidelines from time to time as we learn. We also draw
inspiration from other successful open-source projects and adapt the best ideas for our purposes.

The goals of having these guidelines are fourfold:

1. To maintain a **secure and reliable codebase**. This is paramount.
2. To maintain **high-quality code**. This means thinking hard about
   * fostering a fantastic developer experience,
   * by writing beautiful, ergonomic APIs,
   * using clean patterns,
   * writing excellent documentation,
   * producing very high code coverage on tests,
   * having tests run in a reasonable time,
   * and keeping technical debt to a minimum.
3. To keep the code **open** (as in free).
4. To keep the code open (as in **encouraging**). If we achieve #2, this comes for free. But have fun doing it.

These guidelines are split up into a few main topics:

* [The release process](#the-minotari-release-process) explains how the release schedule for Tari works.
* The [Feature RFC process](#feature-rfc-process) describes how new features get into Tari, from conception
  to implementation.
* [Pull requests](#pull-requests) offers guidelines on how to get your code merged into the code base
  with the minimum of fuss.
* [Code reviews](#code-reviews) are integral to keeping the code secure and performant. This section
  offers tips for effectively preventing bugs from scuttling past your gimlet gaze.
* [Automated CI checks](#automated-ci-checks) lists the checks your PR must pass.

Reporting a security vulnerability is a separate process — please follow the
[Vulnerability Disclosure Policy](SECURITY.md), which also covers the bug bounty programme. **Do not** open a public
issue or PR for a security problem.

# The Minotari release process

The Minotari release process draws inspiration from the Rust [compiler release process] and borrows some of its key
ideas. It is well-suited for a large open-source project that desires a regular release schedule but also needs
the flexibility of having features be "in development" over several cycles.

The authoritative description of the release cycle lives in
[docs/src/branching_releases.md](./docs/src/branching_releases.md). The step-by-step procedures for cutting a release
and for executing a hard fork are in
[docs/Standard Operating Procedure/release.md](./docs/Standard%20Operating%20Procedure/release.md) and
[docs/Standard Operating Procedure/hardfork.md](./docs/Standard%20Operating%20Procedure/hardfork.md).

## Release branches

There are three long-lived branches in the Minotari repo:

* `development` - this is the cuttingest-of-edge branch. Almost all new code comes in via the `development` branch,
  and it is the default branch for pull requests.
* `nextnet` - the code that will become the next `mainnet` release. Feature code from `development` is present here,
  but features that are not yet ready for release stay disabled behind a feature gate.
* `mainnet` - the code on this branch represents the latest "official release". Almost every commit on this branch
  will be tagged with a version number.

Releases are cut by tagging, not by long-lived per-testnet branches: a temporary release branch is created, versioned,
tagged, merged back and deleted. The tag itself is what tells CI which network to build for.

Hotfixes and urgent updates can be made on other branches from time to time. A mainnet hotfix branches from the last
mainnet tag and is then merged into `development`, `nextnet` and `mainnet` — this is the only time code moves
backwards from mainnet into development.

## Networks

* `mainnet` - the "real-money" network.
* `stagenet` - a testnet version of mainnet, built from the same code as `mainnet` to replicate it as closely as
  possible. Network resets (i.e. a new genesis block) should be very rare.
* `nextnet` - built from the `nextnet` branch. This is the last chance to battle-test new features before they hit
  mainnet. Network resets should be somewhat rare.
* `esmeralda` - the most experimental network. Things can break here. We hope they won't, obviously, but this is where
  we should be catching any and all major issues _before_ a feature heads to nextnet. Network resets could occur
  fairly frequently, perhaps several times a year.
* `igor` - a second testnet that sees frequent resets to support a rapid development pace.
* `localnet` - not strictly a testnet; used for local development and integration tests.

Which networks a binary can run is fixed at compile time by `TARI_TARGET_NETWORK`. See
[Choosing a network](README.md#choosing-a-network) in the README.

## Versioning

The release period is **eight weeks**.

At every release period:

* the current `nextnet` version becomes the new `mainnet` version.
* a new `nextnet` version is cut from `development`. Features in the stabilisation cycle are updated to `Testing`.
* features that are ready to go live in the following release are updated to `Active`.
* `development` is bumped to the _next_ version.

Version numbers roughly follow [semver semantics](https://semver.org/), with a pre-release tag that identifies the
intended network:

* Increase MAJOR on each mainnet hard fork.
* Increase MINOR on each release cycle.
* Increase PATCH for hotfixes and all other changes.
* A `-pre.x` suffix (e.g. `v5.7.0-pre.4`) is a development/beta build.
* No suffix (e.g. `v5.6.0`) is a full official build.

There are some implications for this model:

* Hard-fork features MUST hit mainnet well before the feature goes live, so that nodes and miners have time to
  upgrade their software.
* However, sometimes Monero forks happen at short notice, so there may be occasional "quick releases" where we
  execute a release cycle outside the usual cadence.

Note that hard-fork activations are not necessarily triggered by feature gates. Hard forks are usually triggered by
flag days based on block height, via `ConsensusConstants::effective_from_height`. This means that if hard-fork code is
behind a feature gate, that feature needs to be `Active` well before the flag day so that it will trigger at the
appropriate time.

[compiler release process]: https://internals.rust-lang.org/t/release-channels-git-branching-and-the-release-process/1940

## Feature RFC process

Standard issue management is used for bug fixes, performance improvements, and technical debt repayments. Larger
features, and substantial changes to how Tari works, get implemented via the RFC process.

A very rough outline of the flow goes as follows:

- Someone has an idea for an improvement or feature.
- They bring it up and discuss it with the community in the [Discord] #dev channel.
- Someone — usually the leading proponent — writes up a detailed specification of the proposal as an RFC and submits it
  as a PR to the [RFC repo]. At this stage, a number for the RFC will be assigned.
- The community reviews and comments on the RFC.
- After several drafts and revisions, the PR gets merged, and the RFC enters
  ![draft status](./meta/img/status-draft.svg).
- At this point, the RFC can still undergo changes via PRs. The RFC remains in
  ![draft status](./meta/img/status-draft.svg), and the changes are logged in a _Change Log_
  maintained at the bottom of the RFC.
- If someone wants to implement the RFC — usually the RFC author, but it needn't be — they will create a tracking
  issue for the RFC. The tracking issue collects all the conversations around the implementation of the RFC in one
  place. The RFC status will then change to ![WIP status](./meta/img/status-wip.svg). A feature gate
  name will be assigned to the feature at this stage.
- Development for this feature happens behind the feature gate, and PRs are submitted against the `development` branch.
- Once the implementation is complete and active on a testnet, the RFC status can then be changed to
  ![testing status](./meta/img/status-testing.svg).
- The RFC should be updated to match the implementation. The changes must be summarised in the Change Log. Once the
  feature is stabilised and is live on mainnet, the status can be updated to
  ![active status](./meta/img/status-active.svg).

If an RFC ever becomes redundant, DO NOT delete it. Mark the RFC as
![deprecated](./meta/img/status-deprecated.svg) and move it to the _Deprecated_ chapter of the RFC handbook.

The source code must ultimately be the source of truth for the Tari implementation.
If the code and RFC have deviated substantially, mark it ![out of date](./meta/img/status-outofdate.svg).
Then file an issue asking someone to kindly update the RFC and bring it back in line with the code.

Taking on this thankless task is an excellent way for new contributors to learn the code base and quickly add value
to the project!

[Discord]: https://discord.gg/q3Sfzb8S2V
[RFC repo]: https://github.com/tari-project/rfcs

## Pull requests

You're submitting your first, or hundredth, PR to the Tari codebase. Congratulations! The core team could really use
the help. But they also need to be super careful, since the Tari code is managing real money.

It's therefore typical and even expected for pull requests to undergo several revisions before being merged. You
can expect (constructive) feedback and suggestions for improvements that will typically reflect the ideas espoused
in the following set of guidelines.

Open your PR against the `development` branch.

### Sign your commits

**Every commit in your PR must be signed.** This is enforced by CI and is not optional — an unsigned commit will block
the merge. If you have not set this up before, see
[GitHub's guide to commit signature verification](https://docs.github.com/en/authentication/managing-commit-signature-verification).

To sign commits by default:

```bash
git config --global commit.gpgsign true
```

If you have already written unsigned commits, you can sign them retroactively with
`git rebase --exec 'git commit --amend --no-edit -n -S' -i <base-commit>`.

### PRs do one job
[do one job]: #prs-do-one-job

This is really important. A single PR needs to address a single thing. This is not Congress, where bills have
dozens of unrelated things wadded into them in the hope that they're more likely to sneak through with the main
proposal.

Keeping PRs focused on one thing has many benefits:

- It keeps PRs [small](#prs-are-small).
- It maintains focus. It would suck if an entire PR was blocked from being merged because some other unrelated
  change was causing an issue.

If a PR tries to do too much multi-tasking, it will likely be labelled `CR-one_job` and parked. The solution is
simple: break the PR up into 2 or more PRs, each addressing a single issue. Mention in the git comments that you
have done this.

### PRs are small

PRs should be under 400 lines long.

Unit tests do not count towards this line count.

Documentation and RFCs do not contribute to line count. However, RFC PRs should generally not have any code in them
at all (since PRs [do one job]).

In some circumstances, green-field code can break this rule. But then you really MUST make reviewers' lives as easy
as possible by offering multiple commits, reams of documentation, git commit messages and helpful tests.

The 400 line limit represents about an hour of solid code review time. Research indicates that spending more than
this in a single session leads to significantly more bugs scuttling through the door.

If your PR is long, you can expect a reviewer to label it `CR-too_long` and ask you to break it up.

If you really, really can't break it up, then do some [git commit farming] to break the PR up into PR-sized commits.

### Use git messages liberally

Before submitting your PR for the first time, use `git rebase` as described in [git commit farming] to edit and
clean up your commit messages. The messages should supplement the documentation in the code, not just repeat it.
This contains the meta-information: _why_ you are making a change, rather than _what_ it does.

For example, a lousy commit message is:

```text
Refactor signature check
```

We can see that you've changed the signature check code! But we don't know why.

A better message is:

```text
The signature check was checking each signature in the block independently. So for n signatures, the time scaled by
O(n).
We can optimise this check by making use of an aggregated signature check, as described by
[Alice and Bob](https://example.com/paper). This was codified and included in tari_crypto PR #1234.
Changing the signature check to use this improved overall block validation times by 65% (see benchmarks in the
next commit).

All tests still pass but could improve coverage by adding additional tests with large numbers of
signatures (TODO - not in this PR)
```

You may find that a reviewer tags your PR with `CR-insufficient_context`. They are asking you to add
additional context for the change. This can be submitted by tidying up and expanding your git commit messages, the
PR description, or both.

The commit messages are combined to form the pull request's narrative when the PR is merged.
Ideally, if someone reads the `git log` for your PR, they'll have a clear picture of why and how the changes were
introduced.

## Code reviews

Merging into `development` requires:

* at least **one** approving review, and
* an approving review from a **code owner** for any area listed in [CODEOWNERS](./CODEOWNERS) that your PR touches.
  Consensus code, key management, wallet code, and anything under `.github/` or `scripts/` all have designated owners.

Approvals are dismissed when new commits are pushed, so expect to re-request review after making changes.

In practice, security-sensitive changes attract more than the minimum number of reviewers, and you should
actively seek extra eyes on anything touching consensus or cryptography.

When reviewing PRs, here are some guidelines and suggestions to help maintain the safety and quality of code:

* PULL the code and review and test it locally on your machine.
* Do not spend more than 1 hour reviewing a PR at a time.
* Do not review more than 400 LOC in an hour.

If either of these guidelines cannot be met, you may label the PR `CR-too_long` and politely ask the contributor to
revise their PR. Otherwise, take breaks and review the PR over multiple sessions or days.

* Obviously, your main goal of the review is to
  * find bugs,
  * identify edge cases that aren't handled properly,
  * ensure that the stated goal of functions (as per the docstrings and/or RFC) matches the code as written.

All **public** methods and functions must have a decent docstring.
If docstrings are missing, and the purpose of a function or method is not clear, then ask the contributor to
provide them.

* Read the commit messages! They give the context for the change and why (according to the author) they are required.

If there is insufficient justification in your view, or the git messages are too ["haaaands"y], then label the PR
`CR-insufficient_context` and ask the author to address this.

If the PR tends to change things arbitrarily because the "author likes it that way" (e.g. they change from _this_
crate to _that_ crate, or names of things get changed seemingly arbitrarily), we should be wary of merging the PR
unless there are substantial performance, security, or readability reasons. This justification must be
provided in the commit messages. If the author does not provide additional justification AND the other reviewers
agree with your assessment, then this is grounds for rejecting and closing the PR.

There are also some non-goals for a code review:

* Do NOT make comments related to formatting or linting. If our linter doesn't care, neither should you. If the
  linter doesn't like something, the CI tasks will catch it and fail the PR.
* You MAY make Clippy-like suggestions for making code cleaner, more Rust-y, or more readable. But be aware that we
  _do_ run Clippy as part of the CI process and require all errors to be resolved.
* There should be decent test coverage, but 100% coverage is an ideal, not a requirement.

If a PR doesn't have 100% coverage or breaks something non-critical, we may still merge it at our discretion as long as

* issues have been generated and triaged to cover the gaps, and
* the changes are behind a feature gate.

In your code review summary, please mention

* how much effort went into the review (e.g. full review, untested but looks ok — aka "UT ACK", GitHub review), and
* which areas you spent the most time on.

This can help inform other reviewers how much effort they should commit and which areas they should focus on.

Some resources that may be useful:

* https://smartbear.com/learn/code-review/best-practices-for-peer-code-review/
* https://sourcelevel.io/pull-requests-checklists-metrics-and-best-practices-a-definitive-guide
* https://www.atlassian.com/blog/git/written-unwritten-guide-pull-requests

["haaaands"y]: https://xkcd.com/1296/
[git commit farming]: https://www.tari.com/git-farming/ "The Tari Blog: Git farming or, How to get your PRs merged into Tari"

# PR labels

A reviewer may tag a PR with one or more labels during a review. These labels indicate

* that *changes are required* to make reviewing the PR easier (`CR-*`),
* the *category* of the PR (`C-*`),
* the functional *area* of the code it touches (`A-*`),
* where the PR is in the review *process* (`P-*`),
* the *experience* level needed (`E-*`),
* and *warnings* about its blast radius (`W-*`).

The lists below are a snapshot; the
[full label list](https://github.com/tari-project/tari/labels) is authoritative.

### Changes requested (CR)

* `CR-too_long` - Your PR is too long. Follow [these tips](#prs-are-small) to resolve this and resubmit.
* `CR-one_job` - Your PR is not following the [do one job] rule. This should be 2 or more PRs.
* `CR-insufficient_context` - Your PR's commit messages don't provide enough context to justify accepting the change.
* `CR-requires_tests` - The PR does not include tests to prove that the fix solves the problem.

### Categorisation (C)

* `C-bug` - Fixes a bug, typically associated with an issue.
* `C-enhancement` - New feature or request.
* `C-documentation` - Exclusively or dominantly adding to documentation.
* `C-tests` / `C-integration_test` - Adds or changes unit tests / integration tests.
* `C-performance` - Adds no new functionality but improves performance. Benchmarks will be included.
* `C-tech_debt` - Tidies up technical debt.
* `C-proposal` - A new idea written up for community discussion before becoming a formal RFC.
* `C-audit_fix` - Fixes a bug found in an audit.
* `C-depencies` - Dependency update.
* `C-ux` - Improves user experience.
* `C-question` - Further information is requested.

### Area (A)

* `A-base_node`
* `A-comms`
* `A-wallet`
* `A-wallet-ffi`
* `A-mobile_wallet`
* `A-transaction_sending`
* `A-miner`
* `A-security`
* `A-documentation`
* `A-ci`

### Process (P)

* `P-reviews_required` - Requires a review from a lead maintainer to be merged.
* `P-acks_required` - Requires more ACKs or utACKs.
* `P-more_info_needed` - Please provide more information.
* `P-needs_rfc` - Please provide a link to an RFC related to this change.
* `P-designs_required` - Designs required.
* `P-conflicts` - The PR has merge conflicts that need to be resolved.
* `P-clippy_failed` - Clippy has failed.
* `P-do_not_merge` - Not ready for merging.
* `P-controversial` - Requires more attention than simpler issues.
* `P-high-risk` - High risk.
* `P-merge` - Queued for merging.
* `P-duplicate` / `P-wontfix` / `P-archived` - Closed without merging.

### Experience level (E)

* `E-good_first_issue` - Good for newcomers.
* `E-bounty` - Eligible for a bounty.

### Warnings (W)

* `W-consensus_breaking` - Changes consensus rules and requires a hard fork to activate.
* `W-network-breaking` - Contains changes that will not work with existing nodes at a network level.
* `W-transaction-breaking` - Changes data that wallets use to send transactions. This might not cause a hard fork,
  but wallets may not be able to recover funds or interact with each other.
* `W-breaking` - A non-backward-compatible change.

# Automated CI checks

Several checks must pass before a PR will be merged. These run from
[`.github/workflows/ci.yml`](.github/workflows/ci.yml) and
[`.github/workflows/integration_tests.yml`](.github/workflows/integration_tests.yml).

The checks currently required to merge into `development` are: `ci`, `cargo check with stable`, `file licenses`,
`test (mainnet, stagenet)`, `Cucumber tests / Base Layer` and `Cucumber tests / FFI` — plus the signed-commits check.

Most of these have a `cargo` alias so you can reproduce them locally; the aliases are defined in
[`.cargo/config.toml`](.cargo/config.toml).

## Signed commits

Every commit in the PR must have a verified signature. See [Sign your commits](#sign-your-commits) above.

## PR title

PR titles need to conform to the
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) conventions — they are linted with
`commitlint` using `@commitlint/config-conventional`.

If you fall afoul of this, simply edit your PR title, and the check will automatically run again.

## Licence information

All new source files MUST have relevant licence information at the top of the file. Use the BSD-3 licence for all
Tari code. The following text is recommended:

```text
// Copyright The Tari Project
// SPDX-License-Identifier: BSD-3-Clause
```

The check greps for `Copyright.*The Tari Project` (case-insensitive). Scripts, HTML, CSS, binary files, configuration
files, and other non-source types are not subject to this requirement; see the [license check script] for the complete
list of ignored extensions. Files that genuinely cannot carry a header must be added to `.license.ignore`, **sorted
alphabetically**.

Run the check from the repo root (it needs [ripgrep](https://github.com/BurntSushi/ripgrep) installed):

```bash
./scripts/file_license_check.sh
```

[license check script]: ./scripts/file_license_check.sh

## Formatting

Formatting uses nightly-only rustfmt options, so it must be run on a **nightly** toolchain — running `cargo fmt` on
stable will produce different results from CI. The exact nightly CI uses is in the `env.nightly_toolchain` field of the
[CI workflow](.github/workflows/ci.yml).

PRs are checked with:

```bash
cargo +nightly ci-fmt        # equivalent to: cargo fmt --all -- --check
```

If this fails, simply run:

```bash
cargo +nightly ci-fmt-fix    # equivalent to: cargo fmt --all
```

If this does not work, check that rustfmt is installed for that toolchain:
`rustup component add --toolchain nightly rustfmt`.

## Code style

Tari uses Clippy to encourage consistent and idiomatic Rust code.

Clippy does not support project-wide configuration files, so Tari uses
[cargo-lints](https://crates.io/crates/cargo-lints) instead and defines global linting rules in `lints.toml`.

The CI enforces these lints, so check your code with:

```bash
cargo install cargo-lints
cargo ci-clippy              # equivalent to: cargo lints clippy --all-targets --all-features
```

**Note:** generally, you should not put explicit crate-level `deny` attributes in the code, and prefer to put them
in `lints.toml`. You SHOULD however put crate-level `allow` attributes, so that developers running plain
`cargo clippy` will not encounter these warnings.

CI additionally runs `cargo machete` to catch unused dependencies, so remove any dependency you stop using.

## Compilation

CI checks that the workspace compiles on stable and on the minimum supported Rust version declared as
`rust-version` in the workspace `Cargo.toml`. It also builds the Ledger wallet app and the WASM targets. If you raise
the MSRV, that is a deliberate decision that should be called out in your PR description.

```bash
cargo ci-check
```

## Unit tests

All unit tests must pass. CI runs them with [nextest](https://nexte.st/) in release mode, excluding the cucumber
integration tests, across a matrix of all three target networks (`testnet`/esmeralda, `nextnet`/nextnet and
`mainnet`/stagenet), since feature gates change what is compiled:

```bash
cargo install cargo-nextest
cargo ci-test
```

To reproduce a specific matrix leg, set the network before running, e.g.
`TARI_TARGET_NETWORK=mainnet TARI_NETWORK=stagenet cargo ci-test`.

## Integration tests

The cucumber integration tests are a required check. They need the release binaries to be built first:

```bash
cargo build --release
cargo ci-cucumber
```

See [integration_tests/README.md](integration_tests/README.md) for running individual scenarios, filtering by tag, and
collecting logs.
