import { configureStore } from '@reduxjs/toolkit'

import baseNodeReducer from './baseNode'
import walletReducer from './wallet'
import appReducer from './app'
import settingsReducer from './settings'

// exported for tests
export const rootReducer = {
  app: appReducer,
  baseNode: baseNodeReducer,
  wallet: walletReducer,
  settings: settingsReducer,
}

export const store = configureStore({
  reducer: rootReducer,
})

export type RootState = ReturnType<typeof store.getState>

export type AppDispatch = typeof store.dispatch
