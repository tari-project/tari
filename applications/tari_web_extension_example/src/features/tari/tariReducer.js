import { createAsyncThunk, createSlice } from "@reduxjs/toolkit";

const initialState = {
  accounts: [],
  status: "initialize",
};

const handleAccountsChanged =
  (dispatch) =>
  ({ accounts }) => {
    dispatch(tariSlice.actions.setConnected(accounts));
  };

const setupCallbacks = async (tari, dispatch) => {
  tari.on("accountChange", handleAccountsChanged(dispatch));
  tari.checkAccounts().then(handleAccountsChanged(dispatch));
};

export const tariCheckConnection = createAsyncThunk(
  "tari/checkConnection",
  async (_, { dispatch }) => {
    let tari = window.tari;
    if (tari) {
      setupCallbacks(tari, dispatch);
    }
    return tari ? "checking" : "not installed";
  }
);

export const tariSlice = createSlice({
  name: "tari",
  initialState,
  reducers: {
    setConnected: (state, action) => {
      const accounts = action.payload;
      state.accounts = accounts;
      if (accounts.length > 0) {
        state.status = "connected";
      } else {
        state.status = "installed";
      }
    },
  },
  extraReducers: (builder) => {
    builder
      .addCase(tariCheckConnection.pending, (state) => {
        state.status = "checking";
      })
      .addCase(tariCheckConnection.fulfilled, (state, action) => {
        state.status = action.payload;
      });
  },
});

export const tariStatusSelector = (state) => state.tari.status;
export const tariAccountsSelector = (state) => state.tari.accounts;

export default tariSlice.reducer;
