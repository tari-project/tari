#!/usr/bin/env bash
# SPDX-License-Identifier: BSD-3-Clause
#
# Starts a minotari_node binary as a localnet node (TCP transport, no Tor, fresh base path), waits until it has
# started and its gRPC port accepts connections, checks it stays alive for a while longer, then stops it.
# MIMALLOC_VERBOSE=1 is set for the node, and the run fails unless mimalloc reports that it initialised: the binary
# is expected to be built with the default `mimalloc` feature.
#
# Usage: scripts/minotari_node_smoke.sh <path to minotari_node>
# Environment: SMOKE_ALIVE_SECONDS (default 30), SMOKE_STARTUP_SECONDS (default 180),
#              SMOKE_GRPC_PORT (default 18999), SMOKE_P2P_PORT (default 18998)
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <path to minotari_node>" >&2
    exit 2
fi
binary="$1"
alive_seconds="${SMOKE_ALIVE_SECONDS:-30}"
startup_seconds="${SMOKE_STARTUP_SECONDS:-180}"
grpc_port="${SMOKE_GRPC_PORT:-18999}"
p2p_port="${SMOKE_P2P_PORT:-18998}"

base_path=$(mktemp -d)
log="${base_path}/node_output.txt"
node_pid=""

stop_node() {
    if [[ -n "${node_pid}" ]] && kill -0 "${node_pid}" 2>/dev/null; then
        kill -TERM "${node_pid}" 2>/dev/null || true
        for _ in $(seq 1 30); do
            if ! kill -0 "${node_pid}" 2>/dev/null; then
                break
            fi
            sleep 1
        done
        if kill -0 "${node_pid}" 2>/dev/null; then
            echo "node did not exit within 30 s of SIGTERM, sending SIGKILL"
            kill -KILL "${node_pid}" 2>/dev/null || true
        fi
    fi
    rm -rf "${base_path}"
}
trap stop_node EXIT

fail() {
    echo "error: $1" >&2
    echo "---- node output ----" >&2
    cat "${log}" >&2 || true
    exit 1
}

grpc_port_open() {
    (exec 3<>"/dev/tcp/127.0.0.1/${grpc_port}") 2>/dev/null
}

MIMALLOC_VERBOSE=1 "${binary}" \
    --network localnet \
    --base-path "${base_path}" \
    --non-interactive-mode \
    --disable-splash-screen \
    --grpc-address "/ip4/127.0.0.1/tcp/${grpc_port}" \
    -p base_node.p2p.transport.type=tcp \
    -p "base_node.p2p.transport.tcp.listener_address=/ip4/127.0.0.1/tcp/${p2p_port}" \
    -p "base_node.p2p.public_addresses=/ip4/127.0.0.1/tcp/${p2p_port}" \
    -p base_node.p2p.allow_test_addresses=true \
    </dev/null >"${log}" 2>&1 &
node_pid=$!
echo "started minotari_node (pid ${node_pid}), base path ${base_path}"

ready=false
for _ in $(seq 1 "${startup_seconds}"); do
    if ! kill -0 "${node_pid}" 2>/dev/null; then
        fail "the node exited during startup"
    fi
    if grep -q "Node started in non-interactive mode" "${log}" && grpc_port_open; then
        ready=true
        break
    fi
    sleep 1
done
if [[ "${ready}" != true ]]; then
    fail "the node did not start and open gRPC port ${grpc_port} within ${startup_seconds} s"
fi
echo "node started and gRPC port ${grpc_port} is open"

for _ in $(seq 1 "${alive_seconds}"); do
    sleep 1
    if ! kill -0 "${node_pid}" 2>/dev/null; then
        fail "the node exited while it should have been running"
    fi
done
echo "node stayed alive for ${alive_seconds} s"

if ! grep -q "mimalloc: process init" "${log}"; then
    fail "mimalloc did not report initialising; the binary is not using mimalloc"
fi
echo "mimalloc is the node's allocator"
echo "smoke test passed"
