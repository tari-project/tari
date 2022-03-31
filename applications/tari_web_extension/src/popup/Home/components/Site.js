// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import "./site.scss";

export default function Site({ name, onClick }) {
  return (
    <div className={`site`} onClick={onClick}>
      {name}
    </div>
  );
}
