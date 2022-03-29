// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import React from "react";
import ReactDOM from "react-dom";
import store from "./redux/store";
import { Provider } from "react-redux";
import App from "./app";
import KeyManagerFactory from "./keymanager";

// key manager example
(async function () {
  const km = await KeyManagerFactory("branch");
  console.log(
    "KeyManager",
    km,
    km.nextKey(),
    km.deriveKey(1),
    km.nextKey(),
    km.deriveKey(2)
  );
})();

ReactDOM.render(
  <Provider store={store}>
    <App />
  </Provider>,
  document.getElementById("root")
);
