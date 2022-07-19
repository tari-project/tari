import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import {
  ContainerStatus,
  ContainersState,
  Container,
  SystemEventAction,
} from './types'
import {
  addStats,
  start,
  startRecipe,
  stop,
  stopRecipe,
  stopByType,
  restart,
} from './thunks'

const getInitialServiceStatus = (
  lastAction: SystemEventAction,
  exitCode?: number,
): ContainerStatus => ({
  timestamp: Date.now(),
  status: lastAction,
  exitCode,
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
    [Container.Loki]: undefined,
    [Container.Grafana]: undefined,
    [Container.Promtail]: undefined,
  },
  pending: [],
  containers: {},
  stats: {},
}

const containersSlice = createSlice({
  name: 'containers',
  initialState,
  reducers: {
    updateStatus: (
      state,
      action: PayloadAction<{
        containerId: string
        action: SystemEventAction
        exitCode?: number
      }>,
    ) => {
      if (!state.containers[action.payload.containerId]) {
        state.containers[action.payload.containerId] = getInitialServiceStatus(
          action.payload.action,
          action.payload.exitCode,
        )
      } else {
        state.containers[action.payload.containerId].status =
          action.payload.action
        state.containers[action.payload.containerId].exitCode =
          action.payload.exitCode
      }

      if (!state.stats) {
        state.stats = {}
      }

      if (!state.stats[action.payload.containerId]) {
        state.stats[action.payload.containerId] = {
          cpu: 0,
          memory: 0,
          network: {
            upload: 0,
            download: 0,
          },
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
      state.pending.push(meta.arg.container)
      state.errors[meta.arg.container] = undefined
    })
    builder.addCase(start.fulfilled, (state, action) => {
      if (!state.containers[action.payload.id]) {
        return
      }

      state.pending = state.pending.filter(p => p !== action.meta.arg.container)
      state.containers[action.payload.id].name = action.meta.arg.container
      state.containers[action.payload.id].eventsChannel =
        action.payload.containerEventsChannel
      state.stats[action.payload.id].unsubscribe =
        action.payload.unsubscribeStats
      state.errors[action.meta.arg.container] = undefined
    })
    builder.addCase(start.rejected, (state, action) => {
      state.errors[action.meta.arg.container] = action.payload
      state.pending = state.pending.filter(p => p !== action.meta.arg.container)
    })

    builder.addCase(stop.pending, (state, { meta }) => {
      state.pending.push(meta.arg)
      state.containers[meta.arg].error = undefined
    })
    builder.addCase(stop.fulfilled, (state, { meta }) => {
      state.pending = state.pending.filter(p => p !== meta.arg)
      state.containers[meta.arg].error = undefined
      const type = state.containers[meta.arg].name
      if (type) {
        state.errors[type] = undefined
      }
    })
    builder.addCase(stop.rejected, (state, action) => {
      state.pending = state.pending.filter(p => p !== action.meta.arg)
      state.containers[action.meta.arg].error = action.payload
    })

    builder.addCase(addStats.fulfilled, (state, action) => {
      const { containerId, stats } = action.payload

      state.stats[containerId].cpu = stats.cpu
      state.stats[containerId].memory = stats.memory
      state.stats[containerId].network = stats.network
    })
  },
})

const { actions: syncActions } = containersSlice
export const actions = {
  start,
  startRecipe,
  stop,
  stopRecipe,
  stopByType,
  restart,
  ...syncActions,
}

export default containersSlice.reducer
