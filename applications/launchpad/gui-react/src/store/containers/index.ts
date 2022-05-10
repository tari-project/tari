import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import {
  StatsEventPayload,
  ContainerStatus,
  ServicesState,
  ContainerId,
  Container,
  SystemEventAction,
} from './types'
import { start, stop } from './thunks'

const getInitialServiceStatus = (
  id: ContainerId,
  lastAction: SystemEventAction,
): ContainerStatus => ({
  id,
  lastAction,
  stats: {
    cpu: 0,
    memory: 0,
    unsubscribe: () => null,
  },
})

export const initialState: ServicesState = {
  pending: [],
  containers: {},
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
      }>,
    ) => {
      const { stats, containerId } = action.payload

      const cs = stats.cpu_stats
      const pcs = stats.precpu_stats
      const cpu_delta = cs.cpu_usage.total_usage - pcs.cpu_usage.total_usage
      const system_cpu_delta = cs.system_cpu_usage - pcs.system_cpu_usage
      const numCpu = cs.online_cpus
      const cpu = (cpu_delta / system_cpu_delta) * numCpu * 100.0

      state.containers[containerId].stats.cpu = cpu

      const ms = stats.memory_stats
      const memory = (ms.usage - (ms.stats.cache || 0)) / (1024 * 1024)

      state.containers[containerId].stats.memory = memory
    },
    updateStatus: (
      state,
      action: PayloadAction<{ containerId: string; action: SystemEventAction }>,
    ) => {
      if (!state.containers[action.payload.containerId]) {
        state.containers[action.payload.containerId] = getInitialServiceStatus(
          action.payload.containerId,
          action.payload.action,
        )
      } else {
        state.containers[action.payload.containerId].lastAction =
          action.payload.action
      }

      switch (action.payload.action) {
        case SystemEventAction.Destroy:
        case SystemEventAction.Die:
          state.containers[action.payload.containerId].stats.unsubscribe()
          delete state.containers[action.payload.containerId]
          break
      }
    },
  },
  extraReducers: builder => {
    builder.addCase(start.pending, (state, { meta }) => {
      state.pending.push(meta.arg as Container)
    })
    builder.addCase(start.fulfilled, (state, action) => {
      if (!state.containers[action.payload.id]) {
        // TODO throw an error - we *must* already have some system event for this container, right?

        return
      }

      state.pending = state.pending.filter(p => p !== action.meta.arg)
      state.containers[action.payload.id].type = action.meta.arg
      state.containers[action.payload.id].stats.unsubscribe =
        action.payload.unsubscribeStats
    })
    builder.addCase(start.rejected, (state, action) => {
      console.log(`ERROR STARTING CONTAINER ${action.meta.arg}`)
      console.log(action.error)
      state.pending = state.pending.filter(p => p !== action.meta.arg)
    })

    builder.addCase(stop.pending, (state, { meta }) => {
      state.pending.push(meta.arg)
    })
    builder.addCase(stop.fulfilled, (state, { meta }) => {
      state.pending = state.pending.filter(p => p !== meta.arg)
    })
    builder.addCase(stop.rejected, (state, action) => {
      console.log(`ERROR STOPPING CONTAINER ${action.meta.arg}`)
      console.log(action.error)
      state.pending = state.pending.filter(p => p !== action.meta.arg)
    })
  },
})

const { actions: syncActions } = servicesSlice
export const actions = { start, stop, ...syncActions }

export default servicesSlice.reducer
