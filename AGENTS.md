# Instructions for automated agents

This file is read by coding and review agents working in this repository. People should start with
[README.md](README.md) and [Contributing.md](Contributing.md).

## Security reviews and findings

- Read [docs/SECURITY_ASSUMPTIONS.md](docs/SECURITY_ASSUMPTIONS.md) before reporting a security finding. It lists the
  trust assumptions the code is built on.
- Do not report behaviour that only follows from a listed assumption. Report it if it falls under that entry's
  "Not covered" line, or if you can show the assumption is wrong; cite the entry id (for example `A-WALLET-01`).
- If your finding depends on an assumption that is not listed, state the assumption in the report.
- Security issues are reported privately as described in [SECURITY.md](SECURITY.md), not in public issues or pull
  requests.

## Changing code

- A change that weakens a listed assumption, or adds a new trust relationship between components, updates
  `docs/SECURITY_ASSUMPTIONS.md` in the same pull request.
- Follow [Contributing.md](Contributing.md) for commit and pull request conventions.
