import { createAsyncThunk } from '@reduxjs/toolkit'

import { RootState } from '..'
import {
  selectServiceStatus,
  selectRunningServices,
} from '../services/selectors'
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

export const stopNode = createAsyncThunk<void, void, { state: RootState }>(
  'baseNode/stopNode',
  async (_, thunkApi) => {
    try {
      await thunkApi.dispatch(servicesActions.stop(Service.BaseNode)).unwrap()

      const rootState = thunkApi.getState()
      const runningServices = selectRunningServices(rootState)
      if (runningServices.length === 1 && runningServices[0] === Service.Tor) {
        await thunkApi.dispatch(servicesActions.stop(Service.Tor)).unwrap()
      }
    } catch (e) {
      return thunkApi.rejectWithValue(e)
    }
  },
)
