import { createAsyncThunk } from '@reduxjs/toolkit'
import { invoke } from '@tauri-apps/api/tauri'

import type { RootState } from '../'

import { Service, ServiceDescriptor } from './types'

export const start = createAsyncThunk<
  { service: Service; descriptor: ServiceDescriptor },
  Service,
  { state: RootState }
>('services/start', async (service, thunkAPI) => {
  try {
    const rootState = thunkAPI.getState()

    const descriptor: ServiceDescriptor = await invoke('start_service', {
      serviceName: service.toString(),
      settings: rootState.settings.serviceSettings,
    })

    return {
      service,
      descriptor,
    }
  } catch (error) {
    return thunkAPI.rejectWithValue(error)
  }
})
