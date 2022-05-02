// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

import { createAsyncThunk, createSlice } from "@reduxjs/toolkit";
import { sendMessagePromise } from "./common";

export const connectSite = createAsyncThunk(
  "sites/connect",
  async ({ site, callback_id }) => {
    const response = await sendMessagePromise({
      action: "tari-connect-site",
      site,
      callback_id,
    });
    return response;
  }
);

export const getConnectedSites = createAsyncThunk("sites/get", async () => {
  return await sendMessagePromise({ action: "tari-get-connected-sites" });
});

export const sitesSlice = createSlice({
  name: "sites",
  initialState: {
    sites: [],
    status: "initialized",
  },
  reducers: {},
  extraReducers(builder) {
    builder
      .addCase(connectSite.fulfilled, (state, action) => {
        state.sites = [...state.sites, action.payload.sites];
      })
      .addCase(getConnectedSites.fulfilled, (state, action) => {
        state.sites = action.payload.sites;
        state.status = state.sites.length > 0 ? "done" : "empty";
      });
  },
});

export const getSitesSelector = (state) => state?.sites?.sites;
export const getSitesStatusSelector = (state) => state?.sites?.status;

export default sitesSlice.reducer;
