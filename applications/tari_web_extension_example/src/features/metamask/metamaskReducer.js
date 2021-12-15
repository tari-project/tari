import detectEthereumProvider from "@metamask/detect-provider";
import { createAsyncThunk, createSlice } from "@reduxjs/toolkit";

const initialState = {
  accounts: [],
  status: "initialize",
};

const handleAccountsChanged = (dispatch) => (accounts) => {
  dispatch(metamaskSlice.actions.setConnected(accounts));
};

const setupCallbacks = async (ethereum, dispatch) => {
  ethereum.on("accountsChanged", handleAccountsChanged(dispatch));
  ethereum
    .request({ method: "eth_accounts" })
    .then(handleAccountsChanged(dispatch))
    .catch((error) => console.error(error));
};

export const metamaskCheckConnection = createAsyncThunk(
  "metamask/checkConnection",
  async (_, { dispatch }) => {
    let ethereum = await detectEthereumProvider();
    if (ethereum) {
      setupCallbacks(ethereum, dispatch);
    }
    return ethereum ? "checking" : "not installed";
  }
);

export const metamaskSlice = createSlice({
  name: "metamask",
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
      .addCase(metamaskCheckConnection.pending, (state) => {
        state.status = "checking";
      })
      .addCase(metamaskCheckConnection.fulfilled, (state, action) => {
        state.status = action.payload;
      });
  },
});

export const metamaskStatusSelector = (state) => state.metamask.status;
export const metamaskAccountsSelector = (state) => state.metamask.accounts;

export default metamaskSlice.reducer;
