//  Copyright 2021. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

import { invoke } from "@tauri-apps/api/tauri";

async function command_assets_create(
  name,
  description,
  image,
  templateIds,
  templateParameters
) {
  console.log("command_assets_create:", templateParameters);
  return await invoke("assets_create", {
    name,
    description,
    image,
    templateIds,
    templateParameters,
  });
}

async function command_assets_list_owned() {
  return await invoke("assets_list_owned", {});
}

async function command_assets_list_registered_assets(offset, count) {
  return await invoke("assets_list_registered_assets", { offset, count });
}

async function command_assets_get_registration(assetPubKey) {
  return await invoke("assets_get_registration", { assetPubKey });
}

async function command_asset_create_initial_checkpoint(assetPubKey) {
  return await invoke("assets_create_initial_checkpoint", {
    assetPubKey,
  });
}

async function command_asset_create_committee_definition(
  assetPubKey,
  committee
) {
  return await invoke("assets_create_committee_definition", {
    assetPubKey,
    committee,
  });
}

async function command_asset_wallets_get_latest_address(assetPublicKey) {
  return await invoke("asset_wallets_get_latest_address", { assetPublicKey });
}

async function command_asset_wallets_create_address(assetPublicKey) {
  return await invoke("asset_wallets_create_address", { assetPublicKey });
}

async function command_asset_wallets_send_to(
  assetPublicKey,
  amount,
  toAddress
) {
  return await invoke("asset_wallets_send_to", {
    assetPublicKey,
    amount,
    toAddress,
  });
}

async function command_next_asset_public_key() {
  return await invoke("next_asset_public_key", {});
}

async function command_tip004_mint_token(assetPublicKey, token) {
  return await invoke("tip004_mint_token", { assetPublicKey, token });
}

async function command_tip004_list_tokens(assetPublicKey) {
  return await invoke("tip004_list_tokens", { assetPublicKey });
}

async function command_tip721_transfer_from(
  assetPublicKey,
  fromAddressId,
  sendToAddress,
  tokenId
) {
  console.log(fromAddressId, sendToAddress, tokenId);
  return await invoke("tip721_transfer_from", {
    assetPublicKey,
    fromAddressId,
    sendToAddress,
    tokenId,
  });
}

async function command_wallets_create(passphrase, name) {
  return await invoke("wallets_create", { passphrase, name });
}

async function command_wallets_unlock(id, passphrase) {
  return await invoke("wallets_unlock", { id, passphrase });
}

async function command_wallets_seed_words(id, passphrase) {
  return await invoke("wallets_seed_words", { id, passphrase });
}

async function command_wallets_list() {
  return await invoke("wallets_list");
}

async function command_asset_wallets_create(assetPublicKey) {
  return await invoke("asset_wallets_create", { assetPublicKey });
}

async function command_asset_wallets_list() {
  return await invoke("asset_wallets_list", {});
}

async function command_create_db() {
  return await invoke("create_db", {});
}

async function command_asset_wallets_get_balance(assetPublicKey) {
  return await invoke("asset_wallets_get_balance", { assetPublicKey });
}

async function command_asset_wallets_get_unspent_amounts() {
  return await invoke("asset_wallets_get_unspent_amounts", {});
}

const commands = {
  command_create_db,
  command_assets_create,
  command_assets_get_registration,
  command_assets_list_owned,
  command_assets_list_registered_assets,
  command_asset_create_initial_checkpoint,
  command_asset_create_committee_definition,
  command_next_asset_public_key,
  command_asset_wallets_create,
  command_asset_wallets_get_balance,
  command_asset_wallets_get_unspent_amounts,
  command_asset_wallets_list,
  command_asset_wallets_get_latest_address,
  command_asset_wallets_create_address,
  command_asset_wallets_send_to,
  command_tip004_mint_token,
  command_tip004_list_tokens,
  command_tip721_transfer_from,
  command_wallets_create,
  command_wallets_list,
  command_wallets_unlock,
  command_wallets_seed_words,
};

export default commands;
