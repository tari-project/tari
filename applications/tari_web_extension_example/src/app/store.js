// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import { configureStore } from "@reduxjs/toolkit";
import metamaskReducer from "../features/metamask/metamaskReducer";
import tariReducer from "../features/tari/tariReducer";

export const store = configureStore({
  reducer: {
    metamask: metamaskReducer,
    tari: tariReducer,
  },
});
