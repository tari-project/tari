// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import React from "react";

export default function Complete() {
  window.close();
  return (
    <div className="screen">
      <div className="caption">Complete</div>
    </div>
  );
}
