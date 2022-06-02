import { createAsyncThunk } from '@reduxjs/toolkit'
import { invoke } from '@tauri-apps/api/tauri'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { listen } from '@tauri-apps/api/event'

import type { RootState } from '../'
import { selectServiceSettings } from '../settings/selectors'
import { selectNetwork } from '../baseNode/selectors'

import {
  StatsEventPayload,
  ContainerId,
  Container,
  ContainerStats,
  ServiceDescriptor,
} from './types'
import { selectContainerByType } from './selectors'

export const addStats = createAsyncThunk<
  {
    containerId: ContainerId
    stats: Omit<ContainerStats, 'unsubscribe'>
    network: string
  },
  {
    containerId: ContainerId
    service: Container
    stats: StatsEventPayload
  },
  { state: RootState }
>('containers/stats', async (payload, thunkApi) => {
  const { stats, containerId } = payload
  const rootState = thunkApi.getState()

  const cs = stats.cpu_stats
  const pcs = stats.precpu_stats
  const cpu_delta = cs.cpu_usage.total_usage - pcs.cpu_usage.total_usage
  const system_cpu_delta = cs.system_cpu_usage - pcs.system_cpu_usage
  const numCpu = cs.online_cpus
  const cpu = (cpu_delta / system_cpu_delta) * numCpu * 100.0

  const ms = stats.memory_stats
  const memory = (ms.usage - (ms.stats.cache || 0)) / (1024 * 1024)

  return {
    containerId,
    network: selectNetwork(rootState),
    stats: {
      cpu,
      memory,
      timestamp: stats.read,
    },
  }
})

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
        thunkApi.dispatch(
          addStats({
            containerId: descriptor.id,
            service,
            stats: statsEvent.payload,
          }),
        )
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
      const containerStats = rootState.containers.stats[containerId]

      containerStats.unsubscribe()
      await invoke('stop_service', {
        serviceName: (containerStatus.type || '').toString(),
      })
    } catch (error) {
      return thunkApi.rejectWithValue(error)
    }
  },
)

export const stopByType = createAsyncThunk<
  void,
  Container,
  { state: RootState }
>('containers/stopByType', async (containerType, thunkApi) => {
  try {
    const rootState = thunkApi.getState()
    const container = selectContainerByType(containerType)(rootState)

    if (container && container.containerId) {
      await thunkApi.dispatch(stop(container.containerId))
    }
  } catch (error) {
    return thunkApi.rejectWithValue(error)
  }
})
