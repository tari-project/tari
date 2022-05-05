import { createAsyncThunk } from '@reduxjs/toolkit'

import { RootState } from '..'
import { selectServiceStatus } from '../services/selectors'
import { actions as servicesActions } from '../services'
import { Service } from '../services/types'

export const startNode = createAsyncThunk<void, void, { state: RootState }>(
  'baseNode/startNode',
  async (_, thunkApi) => {
    try {
      const rootState = thunkApi.getState()
      const torStatus = selectServiceStatus(Service.Tor)(rootState)

      if (!torStatus.running && !torStatus.pending) {
        await thunkApi.dispatch(servicesActions.start(Service.Tor)).unwrap()
      }

      await thunkApi.dispatch(servicesActions.start(Service.BaseNode)).unwrap()
    } catch (e) {
      return thunkApi.rejectWithValue(e)
    }
  },
)

export const stopNode = createAsyncThunk<void, void>(
  'baseNode/stopNode',
  async (_, thunkApi) => {
    try {
      await thunkApi.dispatch(servicesActions.stop(Service.BaseNode)).unwrap()
      await thunkApi.dispatch(servicesActions.stop(Service.Tor)).unwrap()
    } catch (e) {
      return thunkApi.rejectWithValue(e)
    }
  },
)
