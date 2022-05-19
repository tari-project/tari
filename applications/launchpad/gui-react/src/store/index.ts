import { configureStore } from '@reduxjs/toolkit'
import devToolsEnhancer from 'remote-redux-devtools'

import appReducer from './app'
import settingsReducer from './settings'
import baseNodeReducer from './baseNode'
import miningReducer from './mining'
import walletReducer from './wallet'
import containersReducer from './containers'

// exported for tests
export const rootReducer = {
  app: appReducer,
  baseNode: baseNodeReducer,
  mining: miningReducer,
  wallet: walletReducer,
  settings: settingsReducer,
  containers: containersReducer,
}

export const store = configureStore({
  reducer: rootReducer,
  enhancers: [
    devToolsEnhancer({
      name: 'Tari Launchpad',
      // realtime: true, // enables devtools on production
      hostname: 'localhost',
      port: 8000,
    }),
  ],
})

export type RootState = ReturnType<typeof store.getState>

export const selectRootState = (rootState: RootState) => rootState

export type AppDispatch = typeof store.dispatch
