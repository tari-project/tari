import { configureStore } from '@reduxjs/toolkit'

import appReducer from './app'
import baseNodeReducer from './baseNode'
import miningReducer from './mining'
import walletReducer from './wallet'
import servicesReducer from './services'

// exported for tests
export const rootReducer = {
  app: appReducer,
  baseNode: baseNodeReducer,
  mining: miningReducer,
  wallet: walletReducer,
  settings: settingsReducer,
  services: servicesReducer,
}

export const store = configureStore({
  reducer: rootReducer,
})

export type RootState = ReturnType<typeof store.getState>

export type AppDispatch = typeof store.dispatch
