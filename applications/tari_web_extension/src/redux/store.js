// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import { configureStore } from "@reduxjs/toolkit";
import accountSlice from "./accountSlice";
import assetsSlice from "./assetsSlice";
import sitesSlice from "./sitesSlice";

let store = configureStore({
  reducer: { accounts: accountSlice, assets: assetsSlice, sites: sitesSlice },
});

export default store;
