// Copyright 2026. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! Checks which global allocator the built `minotari_node` binary uses.
//!
//! `#[global_allocator]` is set in the binary's `main.rs`, so a unit test or anything that links the `minotari_node`
//! library runs on the system allocator regardless of features. This test runs the binary itself instead. With
//! `MIMALLOC_VERBOSE=1`, mimalloc writes `mimalloc: process init` to stderr when it initialises; the system allocator
//! and the dhat allocator write nothing of the kind.

use std::process::Command;

/// Runs `minotari_node --version` with mimalloc's verbose output switched on and returns its stderr.
fn version_stderr_with_mimalloc_verbose() -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_minotari_node"))
        .arg("--version")
        .env("MIMALLOC_VERBOSE", "1")
        .output()
        .expect("failed to run the minotari_node binary");
    assert!(
        output.status.success(),
        "`minotari_node --version` failed: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
#[cfg(all(feature = "mimalloc", not(feature = "dhat-heap")))]
fn binary_uses_mimalloc() {
    let stderr = version_stderr_with_mimalloc_verbose();
    assert!(
        stderr.contains("mimalloc: process init"),
        "expected mimalloc to initialise in the minotari_node binary, stderr was:\n{stderr}"
    );
}

#[test]
#[cfg(not(all(feature = "mimalloc", not(feature = "dhat-heap"))))]
fn binary_does_not_use_mimalloc() {
    let stderr = version_stderr_with_mimalloc_verbose();
    assert!(
        !stderr.contains("mimalloc:"),
        "expected the minotari_node binary not to use mimalloc, stderr was:\n{stderr}"
    );
}
