import { createAsyncThunk } from '@reduxjs/toolkit'

import { RootState } from '..'
import {
  selectContainerStatus,
  selectRunningContainers,
} from '../containers/selectors'
import { actions as containersActions } from '../containers'
import { Container } from '../containers/types'

import { selectContainerStatuses } from './selectors'

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
      const rootState = thunkApi.getState()
      const [torContainerStatus, baseNodeContainerStatus] =
        selectContainerStatuses(rootState)

      thunkApi.dispatch(containersActions.stop(baseNodeContainerStatus.id))

      const runningContainers = selectRunningContainers(rootState)
      const otherServicesRunning = runningContainers.some(
        rc => rc !== Container.Tor && rc !== Container.BaseNode,
      )
      if (!otherServicesRunning) {
        thunkApi.dispatch(containersActions.stop(torContainerStatus.id))
      }
    } catch (e) {
      return thunkApi.rejectWithValue(e)
    }
  },
)
