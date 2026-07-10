#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALLER="${SCRIPT_DIR}/install_minotari_ledger.py"
MIN_PYTHON="3.9"

if [[ ! -f "${INSTALLER}" ]]; then
  echo "install_minotari_ledger.py was not found next to this launcher." >&2
  exit 1
fi

python_is_usable() {
  "$1" -c 'import sys; raise SystemExit(0 if sys.version_info >= (3, 9) else 1)' >/dev/null 2>&1
}

find_python() {
  local candidate
  for candidate in python3 python; do
    if command -v "${candidate}" >/dev/null 2>&1; then
      local resolved
      resolved="$(command -v "${candidate}")"
      if python_is_usable "${resolved}"; then
        printf '%s\n' "${resolved}"
        return 0
      fi
    fi
  done
  return 1
}

admin_prefix() {
  if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
    return 0
  fi

  if command -v sudo >/dev/null 2>&1; then
    printf 'sudo\n'
    return 0
  fi

  echo "Python ${MIN_PYTHON}+ is required, but no suitable Python was found and sudo is unavailable." >&2
  echo "Install Python ${MIN_PYTHON}+ with your system package manager, then run this launcher again." >&2
  exit 1
}

confirm_install() {
  local command_text="$1"

  if [[ ! -t 0 ]]; then
    echo "Python ${MIN_PYTHON}+ is required. Run this command, then rerun the installer:" >&2
    echo "  ${command_text}" >&2
    exit 1
  fi

  printf 'Python %s+ is required. Run this command now?\n  %s\n[y/N] ' "${MIN_PYTHON}" "${command_text}" >&2
  local answer
  read -r answer
  case "${answer}" in
    y|Y|yes|YES|Yes)
      return 0
      ;;
    *)
      echo "Install Python ${MIN_PYTHON}+ and rerun this launcher." >&2
      exit 1
      ;;
  esac
}

install_python() {
  if command -v brew >/dev/null 2>&1; then
    confirm_install "brew install python@3.12"
    brew install python@3.12
    return
  fi

  local prefix
  prefix="$(admin_prefix)"

  if command -v apt-get >/dev/null 2>&1; then
    confirm_install "${prefix:+${prefix} }apt-get update && ${prefix:+${prefix} }apt-get install -y python3 python3-venv"
    ${prefix:+${prefix}} apt-get update
    ${prefix:+${prefix}} apt-get install -y python3 python3-venv
  elif command -v dnf >/dev/null 2>&1; then
    confirm_install "${prefix:+${prefix} }dnf install -y python3 python3-pip"
    ${prefix:+${prefix}} dnf install -y python3 python3-pip
  elif command -v yum >/dev/null 2>&1; then
    confirm_install "${prefix:+${prefix} }yum install -y python3 python3-pip"
    ${prefix:+${prefix}} yum install -y python3 python3-pip
  elif command -v pacman >/dev/null 2>&1; then
    confirm_install "${prefix:+${prefix} }pacman -Sy --needed python"
    ${prefix:+${prefix}} pacman -Sy --needed python
  elif command -v zypper >/dev/null 2>&1; then
    confirm_install "${prefix:+${prefix} }zypper install -y python3"
    ${prefix:+${prefix}} zypper install -y python3
  elif command -v apk >/dev/null 2>&1; then
    confirm_install "${prefix:+${prefix} }apk add python3 py3-pip"
    ${prefix:+${prefix}} apk add python3 py3-pip
  else
    echo "Python ${MIN_PYTHON}+ is required and no supported package manager was detected." >&2
    echo "Install Python from https://www.python.org/downloads/ and rerun this launcher." >&2
    exit 1
  fi
}

PYTHON_BIN="$(find_python || true)"
if [[ -z "${PYTHON_BIN}" ]]; then
  install_python
  PYTHON_BIN="$(find_python || true)"
fi

if [[ -z "${PYTHON_BIN}" ]]; then
  echo "Python ${MIN_PYTHON}+ is still unavailable after installation." >&2
  exit 1
fi

exec "${PYTHON_BIN}" "${INSTALLER}" "$@"
