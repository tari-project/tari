import { createSlice } from '@reduxjs/toolkit'

import { ServicesState, Service } from './types'
import { start, stop } from './thunks'

export const initialState: ServicesState = {
  services: {},
  servicesStatus: {
    [Service.Tor]: {
      id: '',
      pending: false,
      running: false,
    },
    [Service.BaseNode]: {
      id: '',
      pending: false,
      running: false,
    },
    [Service.Wallet]: {
      id: '',
      pending: false,
      running: false,
    },
    [Service.SHA3Miner]: {
      id: '',
      pending: false,
      running: false,
    },
    [Service.MMProxy]: {
      id: '',
      pending: false,
      running: false,
    },
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
      state.servicesStatus[action.payload.service].pending = false
      state.servicesStatus[action.payload.service].running = true
      state.servicesStatus[action.payload.service].id =
        action.payload.descriptor.id
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
