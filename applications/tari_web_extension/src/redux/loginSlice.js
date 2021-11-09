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
    credentials: undefined,
  },
  reducers: {},
  extraReducers(builder) {
    builder
      .addCase(login.fulfilled, (state, action) => {
        state.credentials = action.payload.token;
      })
      .addCase(refreshLogin.fulfilled, (state, action) => {
        if (action.payload.successful) {
          state.credentials = action.payload.token;
        }
      });
  },
});

export const getCredentials = (state) => state?.login?.credentials;

export default loginSlice.reducer;
