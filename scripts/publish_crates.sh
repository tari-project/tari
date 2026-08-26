#!/usr/bin/env bash
set -e

# The order is important. Dependencies must be published before the crates that depend on them.
PACKAGES=(
    "tari_storage"
    "tari_shutdown"
    "tari_metrics"
    "tari_max_size"
    "tari_script"
    "tari_hashing"
    "tari_jellyfish"
    "tari_comms_rpc_macros"
    "tari_common_sqlite"
    "tari_features"
    "tari_test_utils"
    "tari_common"
    "tari_comms"
    "tari_comms_dht"
    "minotari_ledger_wallet_common"
    "tari_common_types"
    "tari_sidechain"
    "minotari_ledger_wallet_comms"
    "tari_service_framework"
    "tari_transaction_components"
    "tari_transaction_key_manager"
    "tari_node_components"
    "tari_p2p"
    "tari_libtor"
    "tari_mmr"
    "tari_core"
    "minotari_node_wallet_client"
    "minotari_wallet"
    "minotari_app_grpc"
    "minotari_app_utilities"
    "minotari_wallet_grpc_client"
    "minotari_node_grpc_client"
    "minotari_node"
    "minotari_console_wallet"
)

# Print the local version of a package, e.g. "5.7.0-pre.1".
package_version() {
    local package="$1"
    # cargo pkgid prints either "path+file:///path/to/crate#name@1.2.3" or
    # "file:///path/to/crate#name:1.2.3", depending on the cargo version.
    local pkgid
    pkgid=$(cargo pkgid --package "${package}")
    pkgid="${pkgid##*#}"
    case "${pkgid}" in
        *@*) echo "${pkgid##*@}" ;;
        *:*) echo "${pkgid##*:}" ;;
        *) echo "${pkgid}" ;;
    esac
}

# Path of a crate in the sparse crates.io index, see
# https://doc.rust-lang.org/cargo/reference/registry-index.html#index-files
index_path() {
    local name="$1"
    case "${#name}" in
        1) echo "1/${name}" ;;
        2) echo "2/${name}" ;;
        3) echo "3/${name:0:1}/${name}" ;;
        *) echo "${name:0:2}/${name:2:2}/${name}" ;;
    esac
}

# Returns 0 if the given version is already on crates.io, 1 otherwise.
# A failed lookup (404, network problems, etc.) also returns 1, so we fall back
# to attempting the publish and letting cargo tell us the version already exists.
already_published() {
    local package="$1" version="$2" escaped

    # Escape the regex metacharacters that can appear in a semver string.
    escaped=$(sed 's/[.+]/\\&/g' <<<"${version}")

    curl --silent --fail --location --max-time 30 \
        "https://index.crates.io/$(index_path "${package}")" 2>/dev/null |
        grep -q "\"vers\"[[:space:]]*:[[:space:]]*\"${escaped}\""
}

# Runs cargo publish, tolerating the "already exists" error so that re-running
# the script after a partially completed upload does not abort the whole run.
publish() {
    local logfile exit_code

    logfile=$(mktemp)
    # `set -e` does not trigger here: without `pipefail` the pipeline reports
    # tee's exit code, so cargo's real status is read from PIPESTATUS instead.
    cargo publish "$@" 2>&1 | tee "${logfile}"
    exit_code=${PIPESTATUS[0]}

    if [ "${exit_code}" -ne 0 ] && grep -q "already exists on crates.io index" "${logfile}"; then
        echo "Version already exists on crates.io, skipping."
        exit_code=0
    fi

    rm -f "${logfile}"
    return "${exit_code}"
}

for package in "${PACKAGES[@]}"; do
    version=$(package_version "${package}")

    if already_published "${package}" "${version}"; then
        echo "Skipping ${package} ${version}, already published on crates.io."
        continue
    fi

    echo "Dry-run Publishing ${package} ${version}..."
    publish --package "${package}" --dry-run
    echo "Publishing ${package} ${version}..."
    publish --package "${package}"
done

echo "All packages published successfully."
