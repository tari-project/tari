import { createAsyncThunk } from '@reduxjs/toolkit'
import { invoke } from '@tauri-apps/api/tauri'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { listen } from '@tauri-apps/api/event'

import type { RootState } from '../'
import { selectServiceSettings } from '../settings/selectors'

import { selectServiceStatus } from './selectors'
import { ServiceId, Service, ServiceDescriptor } from './types'

export const start = createAsyncThunk<
  { id: ServiceId; unsubscribeStats: UnlistenFn },
  Service,
  { state: RootState }
>('services/start', async (service, thunkApi) => {
  try {
    const rootState = thunkApi.getState()
    const settings = selectServiceSettings(rootState)

    const descriptor: ServiceDescriptor = await invoke('start_service', {
      serviceName: service.toString(),
      settings,
    })

    const unsubscribe = await listen(descriptor.statsEventsName, statsEvent => {
      console.log(statsEvent)
    })

    return {
      id: descriptor.id,
      unsubscribeStats: unsubscribe,
    }
  } catch (error) {
    return thunkApi.rejectWithValue(error)
  }
})

export const stop = createAsyncThunk<void, Service, { state: RootState }>(
  'services/stop',
  async (service, thunkApi) => {
    try {
      const rootState = thunkApi.getState()
      const serviceStatus = selectServiceStatus(service)(rootState)

      serviceStatus.stats.unsubscribe()
      await invoke('stop_service', {
        serviceName: service.toString(),
      })
    } catch (error) {
      return thunkApi.rejectWithValue(error)
    }
  },
)
