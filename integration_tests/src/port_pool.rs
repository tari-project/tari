//   Copyright 2023. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{collections::VecDeque, net::TcpListener, sync::Mutex};

/// A thread-safe port pool that pre-validates port availability at construction time.
///
/// This eliminates the per-scenario cost of random port scanning, which is especially slow
/// when many concurrent scenarios compete for ports. Ports are returned to the pool after
/// a scenario completes (via [`return_port`]).
pub struct PortPool {
    pools: Mutex<PortPoolInner>,
}

struct PortPoolInner {
    /// Pre-validated available ports for base node P2P (18000-18499)
    p2p_ports: VecDeque<u16>,
    /// Pre-validated available ports for gRPC (18500-18999)
    grpc_ports: VecDeque<u16>,
    /// Pre-validated available ports for HTTP (19000-19499)
    http_ports: VecDeque<u16>,
    /// Pre-validated available ports for wallet gRPC (19500-19999)
    wallet_grpc_ports: VecDeque<u16>,
    /// Pre-validated available ports for wallet HTTP (20000-20499)
    wallet_http_ports: VecDeque<u16>,
}

/// A set of ports allocated for a single base node.
#[derive(Debug, Clone)]
pub struct BaseNodePorts {
    pub p2p: u16,
    pub grpc: u16,
    pub http: u16,
}

/// A set of ports allocated for a single wallet.
#[derive(Debug, Clone)]
pub struct WalletPorts {
    pub grpc: u16,
    pub http: u16,
}

impl PortPool {
    /// Create a new port pool, pre-scanning for available ports.
    ///
    /// `capacity` is the number of ports to pre-validate per category.
    /// A typical value is 50-100, covering most test scenarios.
    pub fn new(capacity: usize) -> Self {
        let p2p_ports = scan_available_ports(18000, 18499, capacity);
        let grpc_ports = scan_available_ports(18500, 18999, capacity);
        let http_ports = scan_available_ports(19000, 19499, capacity);
        let wallet_grpc_ports = scan_available_ports(19500, 19999, capacity);
        let wallet_http_ports = scan_available_ports(20000, 20499, capacity);

        println!(
            "PortPool initialized: {} p2p, {} grpc, {} http, {} wallet_grpc, {} wallet_http ports available",
            p2p_ports.len(),
            grpc_ports.len(),
            http_ports.len(),
            wallet_grpc_ports.len(),
            wallet_http_ports.len(),
        );

        Self {
            pools: Mutex::new(PortPoolInner {
                p2p_ports,
                grpc_ports,
                http_ports,
                wallet_grpc_ports,
                wallet_http_ports,
            }),
        }
    }

    /// Allocate a set of ports for a base node.
    ///
    /// Returns `None` if not enough ports are available (pool exhausted).
    pub fn allocate_base_node_ports(&self) -> Option<BaseNodePorts> {
        let mut inner = self.pools.lock().unwrap();
        let p2p = inner.p2p_ports.pop_front()?;
        let grpc = inner.grpc_ports.pop_front()?;
        let http = inner.http_ports.pop_front()?;
        Some(BaseNodePorts { p2p, grpc, http })
    }

    /// Allocate a set of ports for a wallet.
    pub fn allocate_wallet_ports(&self) -> Option<WalletPorts> {
        let mut inner = self.pools.lock().unwrap();
        let grpc = inner.wallet_grpc_ports.pop_front()?;
        let http = inner.wallet_http_ports.pop_front()?;
        Some(WalletPorts { grpc, http })
    }

    /// Return base node ports to the pool for reuse by another scenario.
    pub fn return_base_node_ports(&self, ports: BaseNodePorts) {
        let mut inner = self.pools.lock().unwrap();
        inner.p2p_ports.push_back(ports.p2p);
        inner.grpc_ports.push_back(ports.grpc);
        inner.http_ports.push_back(ports.http);
    }

    /// Return wallet ports to the pool.
    pub fn return_wallet_ports(&self, ports: WalletPorts) {
        let mut inner = self.pools.lock().unwrap();
        inner.wallet_grpc_ports.push_back(ports.grpc);
        inner.wallet_http_ports.push_back(ports.http);
    }
}

/// Scan a port range and return up to `capacity` ports that are currently available.
fn scan_available_ports(start: u16, end: u16, capacity: usize) -> VecDeque<u16> {
    let mut ports = VecDeque::with_capacity(capacity);
    for port in start..=end {
        if ports.len() >= capacity {
            break;
        }
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            ports.push_back(port);
        }
    }
    ports
}

/// Global port pool, initialized once on first access.
static GLOBAL_PORT_POOL: std::sync::OnceLock<PortPool> = std::sync::OnceLock::new();

/// Get the global port pool, initializing it on first access with 80 ports per category.
pub fn global_port_pool() -> &'static PortPool {
    GLOBAL_PORT_POOL.get_or_init(|| PortPool::new(80))
}
