import { createAsyncThunk } from '@reduxjs/toolkit'
import { invoke } from '@tauri-apps/api/tauri'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { listen } from '@tauri-apps/api/event'

import type { RootState } from '../'
import { selectServiceSettings } from '../settings/selectors'
import { selectNetwork } from '../baseNode/selectors'
import { startOfSecond } from '../../utils/Date'
import getStatsRepository from '../../persistence/statsRepository'

import {
  StatsEventPayload,
  ContainerId,
  Container,
  ServiceDescriptor,
  SerializableContainerStats,
} from './types'
import { selectContainerByType, selectRunningContainers } from './selectors'

export const persistStats = createAsyncThunk<
  void,
  {
    service: Container
    timestamp: string
    stats: SerializableContainerStats
  },
  { state: RootState }
>('containers/persistStats', async (payload, thunkApi) => {
  const { service, timestamp, stats } = payload

  const rootState = thunkApi.getState()
  const configuredNetwork = selectNetwork(rootState)

  const repository = getStatsRepository()
  await repository.add(configuredNetwork, service, timestamp, stats)
})

export const addStats = createAsyncThunk<
  {
    containerId: ContainerId
    stats: SerializableContainerStats
  },
  {
    containerId: ContainerId
    service: Container
    stats: StatsEventPayload
  },
  { state: RootState }
>('containers/stats', async (payload, thunkApi) => {
  const { stats, containerId, service } = payload
  const rootState = thunkApi.getState()

  if (!rootState.containers.stats || !rootState.containers.stats[containerId]) {
    throw new Error('stats for this container dont exist yet')
  }

  const cs = stats.cpu_stats
  const pcs = stats.precpu_stats
  const cpu_delta = cs.cpu_usage.total_usage - pcs.cpu_usage.total_usage
  const system_cpu_delta = cs.system_cpu_usage - pcs.system_cpu_usage
  const numCpu = cs.online_cpus
  const cpu = (cpu_delta / system_cpu_delta) * numCpu * 100.0

  const ms = stats.memory_stats
  const memory = (ms.usage - (ms.stats.cache || 0)) / (1024 * 1024)
  const network = Object.values(stats.networks).reduce(
    (accu, current) => ({
      upload: accu.upload + current.tx_bytes,
      download: accu.download + current.rx_bytes,
    }),
    { upload: 0, download: 0 },
  )

  const currentStats = {
    cpu,
    memory,
    network,
  }
  const secondTimestamp = startOfSecond(new Date(stats.read)).toISOString()
  thunkApi.dispatch(
    persistStats({
      service,
      timestamp: secondTimestamp,
      stats: currentStats,
    }),
  )

  return {
    containerId,
    stats: currentStats,
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

export const restart = createAsyncThunk<void, void, { state: RootState }>(
  'containers/restart',
  async (_, thunkApi) => {
    try {
      const { dispatch } = thunkApi
      const rootState = thunkApi.getState()
      const runningContainers = selectRunningContainers(rootState)

      // Stop all containers first:
      const stopPromises = runningContainers.map(c => {
        return dispatch(stopByType(c))
      })

      await Promise.all(stopPromises)

      // Start containers:
      const startPromises = runningContainers.map(c => {
        return dispatch(start(c))
      })

      await Promise.all(startPromises)
    } catch (error) {
      return thunkApi.rejectWithValue(error)
    }
  },
)
