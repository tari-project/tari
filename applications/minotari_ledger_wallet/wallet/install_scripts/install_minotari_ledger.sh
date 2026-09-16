#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALLER="${SCRIPT_DIR}/install_minotari_ledger.py"

if [[ ! -f "${INSTALLER}" ]]; then
  echo "install_minotari_ledger.py was not found next to this launcher." >&2
  exit 1
fi

# The installer declares its Ledger tooling inline (PEP 723). `uv run` provisions
# an isolated Python plus those dependencies on demand, so no system Python or pip
# setup is required.
if ! command -v uv >/dev/null 2>&1; then
  echo "==> 'uv' was not found; installing it from https://astral.sh/uv for isolated execution..." >&2
  if command -v curl >/dev/null 2>&1; then
    curl -LsSf https://astral.sh/uv/install.sh | sh
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- https://astral.sh/uv/install.sh | sh
  else
    echo "Neither curl nor wget is available to install uv." >&2
    echo "Install uv from https://docs.astral.sh/uv/ and rerun this launcher." >&2
    exit 1
  fi

  # Make uv available in this shell session if the installer dropped an env file.
  for env_file in "${HOME}/.local/bin/env" "${HOME}/.cargo/env"; do
    if [[ -f "${env_file}" ]]; then
      # shellcheck disable=SC1090
      . "${env_file}"
    fi
  done

  # Fall back to the default install locations if uv is still not on PATH.
  if ! command -v uv >/dev/null 2>&1; then
    export PATH="${HOME}/.local/bin:${HOME}/.cargo/bin:${PATH}"
  fi
fi

if ! command -v uv >/dev/null 2>&1; then
  echo "uv is still unavailable after installation. Open a new terminal and rerun," >&2
  echo "or install uv manually from https://docs.astral.sh/uv/." >&2
  exit 1
fi

exec uv run "${INSTALLER}" "$@"
