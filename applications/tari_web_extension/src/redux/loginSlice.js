import { createAsyncThunk, createSlice } from "@reduxjs/toolkit";
import { sendMessagePromise } from "./common";

export const login = createAsyncThunk(
  "user/login",
  async ({ username, password }) => {
    const response = await sendMessagePromise({
      action: "tari-login",
      username: username,
      password: password,
    });
    return response;
  }
);

export const refreshLogin = createAsyncThunk("user/refresh-login", async () => {
  const response = await sendMessagePromise({
    action: "tari-login-refresh",
  });
  return response;
});

export const loginSlice = createSlice({
  name: "login",
  initialState: {
    called: false,
    credentials: undefined,
  },
  reducers: {},
  extraReducers(builder) {
    builder
      .addCase(login.fulfilled, (state, action) => {
        state.credentials = action.payload.token;
      })
      .addCase(refreshLogin.fulfilled, (state, action) => {
        state.called = true;
        state.credentials = action.payload.token;
      })
      .addCase(refreshLogin.rejected, (state, action) => {
        state.called = true;
      });
  },
});

export const getCredentialsCalled = (state) => state?.login?.called;
export const getCredentials = (state) => state?.login?.credentials;

export default loginSlice.reducer;
