// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import "./asset.scss";

export default function Asset({ name, selected, onClick }) {
  return (
    <div className={`asset ${selected && "selected"}`} onClick={onClick}>
      {name}
    </div>
  );
}
