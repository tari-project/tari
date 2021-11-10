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

async function command_assets_create(name, description, image) {
  return await invoke("assets_create", { name, description, image });
}

async function command_assets_list_owned() {
  return await invoke("assets_list_owned", {});
}

async function command_assets_list_registered_assets(offset, count) {
  return await invoke("assets_list_registered_assets", { offset, count });
}

async function command_asset_issue_simple_tokens(
  assetPubKey,
  numTokens,
  committee
) {
  return await invoke("assets_issue_simple_tokens", {
    assetPubKey,
    numTokens,
    committee,
  });
}

async function command_accounts_create(assetPubKey) {
  return await invoke("accounts_create", {assetPubKey});
}
const commands = {
  command_assets_create,
  command_assets_list_owned,
  command_assets_list_registered_assets,
  command_asset_issue_simple_tokens,
  command_accounts_create
};

export default commands;
