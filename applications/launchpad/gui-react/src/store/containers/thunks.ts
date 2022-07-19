import { createAsyncThunk } from '@reduxjs/toolkit'
import { invoke } from '@tauri-apps/api/tauri'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { listen } from '@tauri-apps/api/event'

import type { RootState } from '../'
import { selectServiceSettings } from '../settings/selectors'
import { selectNetwork } from '../baseNode/selectors'
import { selectContainerStatus } from '../containers/selectors'
import { ContainerStatusDto } from '../containers/types'
import { selectRecipe } from '../dockerImages/selectors'
import { startOfSecond } from '../../utils/Date'
import getStatsRepository from '../../persistence/statsRepository'
import { ContainerName } from '../../types/general'

import {
  StatsEventPayload,
  ContainerId,
  Container,
  ServiceDescriptor,
  SerializableContainerStats,
} from './types'
import { selectContainer, selectRunningContainers } from './selectors'

export const persistStats = createAsyncThunk<
  void,
  {
    container: ContainerName
    timestamp: string
    stats: SerializableContainerStats
  },
  { state: RootState }
>('containers/persistStats', async (payload, thunkApi) => {
  const { container, timestamp, stats } = payload

  const rootState = thunkApi.getState()
  const configuredNetwork = selectNetwork(rootState)

  const repository = getStatsRepository()
  await repository.add(configuredNetwork, container, timestamp, stats)
})

export const extractStatsFromEvent = (
  stats: StatsEventPayload,
): SerializableContainerStats & { timestamp: string } => {
  const secondTimestamp = startOfSecond(new Date(stats.read)).toISOString()

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

  return {
    cpu,
    memory,
    network,
    timestamp: secondTimestamp,
  }
}

export const addStats = createAsyncThunk<
  {
    containerId: ContainerId
    stats: SerializableContainerStats
  },
  {
    containerId: ContainerId
    container: ContainerName
    stats: StatsEventPayload
  },
  { state: RootState }
>('containers/stats', async (payload, thunkApi) => {
  const { stats, containerId, container } = payload
  const rootState = thunkApi.getState()

  if (!rootState.containers.stats || !rootState.containers.stats[containerId]) {
    throw new Error('stats for this container dont exist yet')
  }

  const currentStats = extractStatsFromEvent(stats)
  thunkApi.dispatch(
    persistStats({
      container,
      timestamp: currentStats.timestamp,
      stats: currentStats,
    }),
  )

  return {
    containerId,
    stats: currentStats,
  }
})

export const start = createAsyncThunk<
  {
    id: ContainerId
    containerEventsChannel: string
    unsubscribeStats: UnlistenFn
  },
  { container: ContainerName },
  { state: RootState }
>('containers/start', async ({ container }, thunkApi) => {
  try {
    const rootState = thunkApi.getState()
    const settings = selectServiceSettings(rootState)

    const descriptor: ServiceDescriptor = await invoke('start_service', {
      serviceName: container,
      settings,
    })

    const unsubscribe = await listen(
      descriptor.statsEventsName,
      (statsEvent: { payload: StatsEventPayload }) => {
        thunkApi.dispatch(
          addStats({
            containerId: descriptor.id,
            container,
            stats: statsEvent.payload,
          }),
        )
      },
    )

    return {
      id: descriptor.id,
      containerEventsChannel: descriptor.statsEventsName,
      unsubscribeStats: unsubscribe,
    }
  } catch (error) {
    return thunkApi.rejectWithValue(error)
  }
})

export const startRecipe = createAsyncThunk<
  void,
  { containerName: ContainerName },
  { state: RootState }
>('containers/startRecipe', async ({ containerName }, thunkApi) => {
  try {
    const rootState = thunkApi.getState()
    const recipe = selectRecipe(containerName)(rootState)

    const recipePromises = [...recipe]
      .reverse()
      .map(part => {
        const status = selectContainerStatus(part)(rootState)
        if (!status.running && !status.pending) {
          return thunkApi.dispatch(start({ container: part })).unwrap()
        }

        return false
      })
      .filter(Boolean)

    await Promise.all(recipePromises)
  } catch (e) {
    return thunkApi.rejectWithValue(e)
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
        serviceName: (containerStatus.name || '').toString(),
      })
    } catch (error) {
      return thunkApi.rejectWithValue(error)
    }
  },
)

export const stopRecipe = createAsyncThunk<
  void,
  ContainerName,
  { state: RootState }
>('containers/stopRecipe', async (containerName, thunkApi) => {
  try {
    const rootState = thunkApi.getState()
    const recipe = selectRecipe(containerName)(rootState)

    const runningContainers = selectRunningContainers(rootState)
    const containersOutsideRecipe = runningContainers.filter(
      rc => !recipe.includes(rc),
    )
    const containersToRemoveFromOtherServicesRecipes = containersOutsideRecipe
      .reduce((accu, current) => {
        const currentRecipe = selectRecipe(current)(rootState)

        const dependedOnAnythingInRecipe = currentRecipe.some(cr =>
          recipe.includes(cr),
        )
        if (dependedOnAnythingInRecipe) {
          return [...accu, ...currentRecipe]
        }

        return accu
      }, [] as ContainerName[])
      .map(part => selectContainerStatus(part)(rootState))
    const currentRecipeContainers = recipe.map(part =>
      selectContainerStatus(part)(rootState),
    )

    const deduplicatedContainersToRemove = new Set<ContainerStatusDto>([
      ...containersToRemoveFromOtherServicesRecipes,
      ...currentRecipeContainers,
    ])

    Array.from(deduplicatedContainersToRemove.values()).forEach(
      (toRemove: ContainerStatusDto) => thunkApi.dispatch(stop(toRemove.id)),
    )
  } catch (e) {
    return thunkApi.rejectWithValue(e)
  }
})

export const stopByType = createAsyncThunk<
  void,
  Container,
  { state: RootState }
>('containers/stopByType', async (containerName, thunkApi) => {
  try {
    const rootState = thunkApi.getState()
    const container = selectContainer(containerName)(rootState)

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
        return dispatch(start({ container: c }))
      })

      await Promise.all(startPromises)
    } catch (error) {
      return thunkApi.rejectWithValue(error)
    }
  },
)
