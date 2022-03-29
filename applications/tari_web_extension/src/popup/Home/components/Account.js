// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import "./account.scss";

export default function Account({ name, onClick }) {
  return (
    <div className={`account`} onClick={onClick}>
      {name}
    </div>
  );
}
