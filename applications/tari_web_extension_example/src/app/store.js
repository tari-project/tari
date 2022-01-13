import { configureStore } from "@reduxjs/toolkit";
import metamaskReducer from "../features/metamask/metamaskReducer";
import tariReducer from "../features/tari/tariReducer";

export const store = configureStore({
  reducer: {
    metamask: metamaskReducer,
    tari: tariReducer,
  },
});
