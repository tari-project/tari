import { combineReducers } from 'redux'
import { configureStore } from '@reduxjs/toolkit'
import { persistStore, persistReducer } from 'redux-persist'
import storage from 'redux-persist/lib/storage'

import appReducer from './app'
import settingsReducer from './settings'
import baseNodeReducer from './baseNode'
import miningReducer from './mining'
import walletReducer from './wallet'
import containersReducer from './containers'
import tbotReducer from './tbot'
import dockerImagesReducer from './dockerImages'
import credentialsReducer from './credentials'
import temporaryReducer from './temporary'

// exported for tests
export const rootReducer = {
  app: appReducer,
  baseNode: baseNodeReducer,
  mining: miningReducer,
  wallet: walletReducer,
  settings: settingsReducer,
  containers: containersReducer,
  tbot: tbotReducer,
  dockerImages: dockerImagesReducer,
  credentials: credentialsReducer,
  temporary: temporaryReducer,
}
const reducer = combineReducers(rootReducer)

const persistConfig = {
  key: 'root',
  storage,
  blacklist: ['containers', 'credentials', 'temporary'],
}

const persistedReducer = persistReducer(persistConfig, reducer)

export const store =
  process.env.NODE_ENV === 'test'
    ? configureStore({
        reducer,
      })
    : configureStore({
        reducer: persistedReducer,
        middleware: getDefaultMiddleware => [
          // Silent console errors about non-serializable data
          ...getDefaultMiddleware({ serializableCheck: false }),
        ],
      })

export const persistor = persistStore(store)

export type RootState = ReturnType<typeof store.getState>

export const selectRootState = (rootState: RootState) => rootState

export type AppDispatch = typeof store.dispatch
