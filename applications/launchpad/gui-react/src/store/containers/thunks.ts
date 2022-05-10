import { createAsyncThunk } from '@reduxjs/toolkit'
import { invoke } from '@tauri-apps/api/tauri'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { listen } from '@tauri-apps/api/event'

import type { RootState } from '../'
import { selectServiceSettings } from '../settings/selectors'

import {
  StatsEventPayload,
  ContainerId,
  Container,
  ServiceDescriptor,
} from './types'

export const start = createAsyncThunk<
  { id: ContainerId; unsubscribeStats: UnlistenFn },
  Container,
  { state: RootState }
>('containers/start', async (service, thunkApi) => {
  try {
    const rootState = thunkApi.getState()
    const settings = selectServiceSettings(rootState)

    const descriptor: ServiceDescriptor = await invoke('start_service', {
      serviceName: service.toString(),
      settings,
    })

    const unsubscribe = await listen(
      descriptor.statsEventsName,
      (statsEvent: { payload: StatsEventPayload }) => {
        thunkApi.dispatch({
          type: 'containers/stats',
          payload: { containerId: descriptor.id, stats: statsEvent.payload },
        })
      },
    )

    return {
      id: descriptor.id,
      unsubscribeStats: unsubscribe,
    }
  } catch (error) {
    return thunkApi.rejectWithValue(error)
  }
})

export const stop = createAsyncThunk<void, ContainerId, { state: RootState }>(
  'containers/stop',
  async (containerId, thunkApi) => {
    try {
      const rootState = thunkApi.getState()
      const containerStatus = rootState.containers.containers[containerId]

      containerStatus.stats.unsubscribe()
      await invoke('stop_service', {
        serviceName: (containerStatus.type || '').toString(),
      })
    } catch (error) {
      return thunkApi.rejectWithValue(error)
    }
  },
)
