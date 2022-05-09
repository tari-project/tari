// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

/* global BigInt */
const wasm = import("./key_manager/index_bg.wasm");

async function KeyManagerFactory(branch = "") {
  const module = await wasm;
  return new KeyManager(module, branch);
}

class KeyManager {
  constructor(module, branch = "") {
    this._module = module;

    const response = module.key_manager_new(branch);
    if (response.success) {
      this._km = response.key_manager;
    } else {
      throw new Error(`Error creating Key Manager: ${response.error}`);
    }
  }

  nextKey() {
    const response = this._module.next_key(this._km);
    if (response.success) {
      this._km = response.key_manager;
      return response.keypair;
    } else {
      throw new Error(`Error in next_key: ${response.error}`);
    }
  }

  deriveKey(index) {
    const response = this._module.derive_key(this._km, BigInt(index));
    if (response.success) {
      this._km = response.key_manager;
      return response.keypair;
    } else {
      throw new Error(`Error in derive_key: ${response.error}`);
    }
  }
}

export default KeyManagerFactory;
