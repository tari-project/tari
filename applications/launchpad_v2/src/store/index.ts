import { configureStore } from '@reduxjs/toolkit'

import baseNodeReducer from '../containers/BaseNodeContainer/slice'

import appReducer from './app'

export const store = configureStore({
  reducer: {
    app: appReducer,
    baseNode: baseNodeReducer,
  },
})

export type RootState = ReturnType<typeof store.getState>

export type AppDispatch = typeof store.dispatch
