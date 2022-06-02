import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import {
  StatsEventPayload,
  ContainerStatus,
  ContainersState,
  ContainerId,
  Container,
  SystemEventAction,
} from './types'
import { start, stop, stopByType } from './thunks'

const getInitialServiceStatus = (
  lastAction: SystemEventAction,
): ContainerStatus => ({
  timestamp: Date.now(),
  status: lastAction,
})

export const initialState: ContainersState = {
  errors: {
    [Container.Tor]: undefined,
    [Container.BaseNode]: undefined,
    [Container.Wallet]: undefined,
    [Container.SHA3Miner]: undefined,
    [Container.MMProxy]: undefined,
    [Container.XMrig]: undefined,
    [Container.Monerod]: undefined,
    [Container.Frontail]: undefined,
  },
  pending: [],
  containers: {},
  stats: {},
  statsHistory: {},
}

const servicesSlice = createSlice({
  name: 'containers',
  initialState,
  reducers: {
    stats: (
      state,
      action: PayloadAction<{
        containerId: ContainerId
        stats: StatsEventPayload
        service: Container
        network: string
      }>,
    ) => {
      const { stats, containerId, network, service } = action.payload

      if (!state.stats || !state.stats[containerId]) {
        return
      }

      const cs = stats.cpu_stats
      const pcs = stats.precpu_stats
      const cpu_delta = cs.cpu_usage.total_usage - pcs.cpu_usage.total_usage
      const system_cpu_delta = cs.system_cpu_usage - pcs.system_cpu_usage
      const numCpu = cs.online_cpus
      const cpu = (cpu_delta / system_cpu_delta) * numCpu * 100.0

      state.stats[containerId].timestamp = stats.read
      state.stats[containerId].cpu = cpu

      const ms = stats.memory_stats
      const memory = (ms.usage - (ms.stats.cache || 0)) / (1024 * 1024)

      state.stats[containerId].memory = memory

      const { unsubscribe: _unsubscribe, ...statsToRemember } =
        state.stats[containerId]
      if (!state.statsHistory[network]) {
        state.statsHistory = {
          [network]: {
            [Container.Tor]: [],
            [Container.BaseNode]: [],
            [Container.Wallet]: [],
            [Container.SHA3Miner]: [],
            [Container.MMProxy]: [],
            [Container.XMrig]: [],
            [Container.Monerod]: [],
            [Container.Frontail]: [],
          },
        }
      }
      state.statsHistory[network][service].push(statsToRemember)
    },
    updateStatus: (
      state,
      action: PayloadAction<{ containerId: string; action: SystemEventAction }>,
    ) => {
      if (!state.containers[action.payload.containerId]) {
        state.containers[action.payload.containerId] = getInitialServiceStatus(
          action.payload.action,
        )
      } else {
        state.containers[action.payload.containerId].status =
          action.payload.action
      }

      if (!state.stats) {
        state.stats = {}
      }

      if (!state.stats[action.payload.containerId]) {
        state.stats[action.payload.containerId] = {
          cpu: 0,
          memory: 0,
          timestamp: '',
          unsubscribe: () => {
            return
          },
        }
      }

      switch (action.payload.action) {
        case SystemEventAction.Destroy:
        case SystemEventAction.Die:
          state.stats[action.payload.containerId].unsubscribe()
          state.stats[action.payload.containerId].cpu = 0
          state.stats[action.payload.containerId].memory = 0
          break
      }
    },
  },
  extraReducers: builder => {
    builder.addCase(start.pending, (state, { meta }) => {
      state.pending.push(meta.arg)
      state.errors[meta.arg] = undefined
    })
    builder.addCase(start.fulfilled, (state, action) => {
      if (!state.containers[action.payload.id]) {
        // TODO throw an error - we *must* already have some system event for this container, right?

        return
      }

      state.pending = state.pending.filter(p => p !== action.meta.arg)
      state.containers[action.payload.id].type = action.meta.arg
      state.stats[action.payload.id].unsubscribe =
        action.payload.unsubscribeStats
      state.errors[action.meta.arg] = undefined
    })
    builder.addCase(start.rejected, (state, action) => {
      state.errors[action.meta.arg] = action.payload
      state.pending = state.pending.filter(p => p !== action.meta.arg)
    })

    builder.addCase(stop.pending, (state, { meta }) => {
      state.pending.push(meta.arg)
      state.containers[meta.arg].error = undefined
    })
    builder.addCase(stop.fulfilled, (state, { meta }) => {
      state.pending = state.pending.filter(p => p !== meta.arg)
      state.containers[meta.arg].error = undefined
      const type = state.containers[meta.arg].type
      if (type) {
        state.errors[type] = undefined
      }
    })
    builder.addCase(stop.rejected, (state, action) => {
      state.pending = state.pending.filter(p => p !== action.meta.arg)
      state.containers[action.meta.arg].error = action.payload
    })
  },
})

const { actions: syncActions } = servicesSlice
export const actions = { start, stop, stopByType, ...syncActions }

export default servicesSlice.reducer
