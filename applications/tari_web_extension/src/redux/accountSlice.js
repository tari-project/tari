// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import { createAsyncThunk, createSlice } from "@reduxjs/toolkit";
import { sendMessagePromise } from "./common";

export const createAccount = createAsyncThunk(
  "account/create",
  async (password) => {
    const response = await sendMessagePromise({
      action: "tari-create-account",
      password: password,
    });
    return response;
  }
);

export const getAccounts = createAsyncThunk("account/get", async () => {
  return (await sendMessagePromise({ action: "tari-get-accounts" })).payload;
});

export const accountSlice = createSlice({
  name: "accounts",
  initialState: {
    accounts: [],
    status: "initialized",
  },
  reducers: {},
  extraReducers(builder) {
    builder
      .addCase(createAccount.fulfilled, (state, action) => {
        state.status = "done";
        state.accounts = action.payload.accounts;
      })
      .addCase(getAccounts.fulfilled, (state, action) => {
        if (action.payload.accounts.length > 0) {
          state.status = "done";
          state.accounts = action.payload.accounts;
        } else {
          state.status = "empty";
          state.accounts = [];
        }
      })
      .addCase(getAccounts.rejected, (state, action) => {
        console.log("tari-get-accoungs rejected");
      });
  },
});

export const getAccountsSelector = (state) => state?.accounts?.accounts;
export const getAccountsStatusSelector = (state) => state?.accounts?.status;

export default accountSlice.reducer;
