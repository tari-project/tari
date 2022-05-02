// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import React from "react";
import { Link } from "react-router-dom";

export default function SetupWallet() {
  return (
    <div className="screen">
      <div className="caption">New to Tari?</div>
      <div className="row">
        <Link
          className="button"
          to="../improve"
          state={{ next: "../import-wallet" }}
        >
          Import Wallet
        </Link>
        <Link
          className="button"
          to="../improve"
          state={{ next: "../create-wallet" }}
        >
          Create a Wallet
        </Link>
      </div>
    </div>
  );
}
