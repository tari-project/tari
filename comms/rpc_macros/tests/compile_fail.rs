// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Misuse of `#[tari_rpc]` must be a compile error with a useful message, never a panic or silently wrong code.

#[test]
fn invalid_rpc_traits_do_not_compile() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
}
