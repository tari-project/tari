//   Copyright 2022. The Tari Project
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

use cucumber::{given, when, then};
use tari_integration_tests::TariWorld;

#[given(expr = "I have a wallet named '{string}'")]
async fn i_have_a_wallet(world: &mut TariWorld, wallet_name: String) {
    world.create_wallet(&wallet_name).await.unwrap();
}

#[when(expr = "I send {int} tari from '{string}' to '{string}'")]
async fn i_send_tari(world: &mut TariWorld, amount: u64, from_wallet: String, to_wallet: String) {
    let source_wallet = world.get_wallet(&from_wallet).unwrap();
    let destination_wallet = world.get_wallet(&to_wallet).unwrap();
    source_wallet.send_tari(amount, &destination_wallet.address()).await.unwrap();
}

#[then(expr = "'{string}' should have {int} tari")]
async fn wallet_should_have_tari(world: &mut TariWorld, wallet_name: String, expected_amount: u64) {
    let wallet = world.get_wallet(&wallet_name).unwrap();
    assert_eq!(wallet.balance().await.unwrap(), expected_amount);
}
