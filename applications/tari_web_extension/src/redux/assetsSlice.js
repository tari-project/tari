import { createAsyncThunk, createSlice } from "@reduxjs/toolkit";
import { sendMessagePromise } from "./common";

export const loadAssets = createAsyncThunk("assets/get", async () => {
  const response = await sendMessagePromise({
    action: "tari-get-assets",
  });
  return response;
});

export const selectAsset = createAsyncThunk("assets/select", async (name) => {
  const response = await sendMessagePromise({
    action: "tari-select-asset",
    name: name,
  });
  return response;
});

export const assetsSlice = createSlice({
  name: "assets",
  initialState: {
    assets: [],
    selected: undefined,
  },
  reducers: {},
  extraReducers(builder) {
    builder
      .addCase(loadAssets.fulfilled, (state, action) => {
        state.assets = action.payload.assets;
        state.selected = action.payload.selected;
      })
      .addCase(selectAsset.fulfilled, (state, action) => {
        state.selected = action.payload.selected;
      });
  },
});

export const getAssets = (state) => state?.assets.assets || [];
export const getSelectedAsset = (state) => state?.assets.selected;

export default assetsSlice.reducer;
