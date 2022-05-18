import { combineReducers } from 'redux'
import { configureStore } from '@reduxjs/toolkit'
import devToolsEnhancer from 'remote-redux-devtools'
import { persistStore, persistReducer } from 'redux-persist'
import storage from 'redux-persist/lib/storage'

import appReducer from './app'
import settingsReducer from './settings'
import baseNodeReducer from './baseNode'
import miningReducer from './mining'
import walletReducer from './wallet'
import containersReducer from './containers'
import tbotReducer from './tbot'

// exported for tests
export const rootReducer = {
  app: appReducer,
  baseNode: baseNodeReducer,
  mining: miningReducer,
  wallet: walletReducer,
  settings: settingsReducer,
  containers: containersReducer,
  tbot: tbotReducer,
}
const reducer = combineReducers(rootReducer)

const persistConfig = {
  key: 'root',
  storage,
  blacklist: ['containers'],
}
const persistedReducer = persistReducer(persistConfig, reducer)

export const store = configureStore({
  reducer: persistedReducer,
  enhancers: [
    devToolsEnhancer({
      name: 'Tari Launchpad',
      // realtime: true, // enables devtools on production
      hostname: 'localhost',
      port: 8000,
    }),
  ],
})

export const persistor = persistStore(store)

export type RootState = ReturnType<typeof store.getState>

export const selectRootState = (rootState: RootState) => rootState

export type AppDispatch = typeof store.dispatch
