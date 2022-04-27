import { configureStore } from '@reduxjs/toolkit'

import baseNodeReducer from './baseNode'
import walletReducer from './wallet'
import appReducer from './app'

export const store = configureStore({
  reducer: {
    app: appReducer,
    baseNode: baseNodeReducer,
    wallet: walletReducer,
  },
})

export type RootState = ReturnType<typeof store.getState>

export type AppDispatch = typeof store.dispatch
