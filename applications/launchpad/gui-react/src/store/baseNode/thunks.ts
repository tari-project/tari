import { createAsyncThunk } from '@reduxjs/toolkit'

import { RootState } from '..'
import {
  selectContainerStatus,
  selectRunningContainers,
} from '../containers/selectors'
import { actions as containersActions } from '../containers'
import { Container } from '../containers/types'

export const startNode = createAsyncThunk<void, void, { state: RootState }>(
  'baseNode/startNode',
  async (_, thunkApi) => {
    try {
      const rootState = thunkApi.getState()
      const torStatus = selectContainerStatus(Container.Tor)(rootState)

      if (!torStatus.running && !torStatus.pending) {
        await thunkApi.dispatch(containersActions.start(Container.Tor)).unwrap()
      }

      await thunkApi
        .dispatch(containersActions.start(Container.BaseNode))
        .unwrap()
    } catch (e) {
      return thunkApi.rejectWithValue(e)
    }
  },
)

export const stopNode = createAsyncThunk<void, void, { state: RootState }>(
  'baseNode/stopNode',
  async (_, thunkApi) => {
    try {
      await thunkApi
        .dispatch(containersActions.stop(Container.BaseNode))
        .unwrap()

      const rootState = thunkApi.getState()
      const runningServices = selectRunningContainers(rootState)
      if (
        runningServices.length === 1 &&
        runningServices[0] === Container.Tor
      ) {
        await thunkApi.dispatch(containersActions.stop(Container.Tor)).unwrap()
      }
    } catch (e) {
      return thunkApi.rejectWithValue(e)
    }
  },
)
