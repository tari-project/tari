import { configureStore } from "@reduxjs/toolkit";
import assetsSlice from "./assetsSlice";
import loginReducer from "./loginSlice";

let store = configureStore({
  reducer: { login: loginReducer, assets: assetsSlice },
});

export default store;
