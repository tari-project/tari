import { createSlice } from '@reduxjs/toolkit'

import { ServiceStatus, ServicesState, Service } from './types'
import { start, stop } from './thunks'

const getInitialServiceStatus = (): ServiceStatus => ({
  id: '',
  pending: false,
  running: false,
  stats: {
    cpu: -1,
    memory: -1,
    unsubscribe: () => null,
  },
})

export const initialState: ServicesState = {
  services: {},
  servicesStatus: {
    [Service.Tor]: getInitialServiceStatus(),
    [Service.BaseNode]: getInitialServiceStatus(),
    [Service.Wallet]: getInitialServiceStatus(),
    [Service.SHA3Miner]: getInitialServiceStatus(),
    [Service.MMProxy]: getInitialServiceStatus(),
    [Service.XMrig]: getInitialServiceStatus(),
    [Service.Monerod]: getInitialServiceStatus(),
    [Service.Frontail]: getInitialServiceStatus(),
  },
}

const servicesSlice = createSlice({
  name: 'services',
  initialState,
  reducers: {},
  extraReducers: builder => {
    builder.addCase(start.pending, (state, { meta }) => {
      state.servicesStatus[meta.arg].pending = true
      state.servicesStatus[meta.arg].error = undefined
    })
    builder.addCase(start.fulfilled, (state, action) => {
      state.servicesStatus[action.meta.arg].pending = false
      state.servicesStatus[action.meta.arg].running = true
      state.servicesStatus[action.meta.arg].id = action.payload.id
      state.servicesStatus[action.meta.arg].stats.unsubscribe =
        action.payload.unsubscribeStats
    })
    builder.addCase(start.rejected, (state, action) => {
      state.servicesStatus[action.meta.arg].pending = false
      state.servicesStatus[action.meta.arg].running = false
      state.servicesStatus[action.meta.arg].error = action.error.message
      console.log(`ERROR STARTING SERVICE ${action.meta.arg}`)
      console.log(action.error)
    })

    builder.addCase(stop.pending, (state, { meta }) => {
      state.servicesStatus[meta.arg].pending = true
      state.servicesStatus[meta.arg].error = undefined
    })
    builder.addCase(stop.fulfilled, (state, action) => {
      state.servicesStatus[action.meta.arg].pending = false
      state.servicesStatus[action.meta.arg].running = false
    })
    builder.addCase(stop.rejected, (state, action) => {
      state.servicesStatus[action.meta.arg].pending = false
      state.servicesStatus[action.meta.arg].running = false
      state.servicesStatus[action.meta.arg].error = action.error.message
      console.log(`ERROR STOPPING SERVICE ${action.meta.arg}`)
      console.log(action.error)
    })
  },
})

export const actions = { start, stop }

export default servicesSlice.reducer
