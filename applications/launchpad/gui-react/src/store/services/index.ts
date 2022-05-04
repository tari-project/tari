import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import { ServiceDescriptor, ServicesState, Service } from './types'
import { start } from './thunks'

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
      console.log('pending', meta.arg)
      state.servicesStatus[meta.arg].pending = true
      state.servicesStatus[meta.arg].error = ''
    })
    builder.addCase(start.fulfilled, (state, action) => {
      state.servicesStatus[action.payload.service].pending = false
      state.servicesStatus[action.payload.service].id =
        action.payload.descriptor.id

      console.log(action.payload)
    })
    builder.addCase(start.rejected, (state, action) => {
      console.log('ERROR STARING')
      console.log(action.error)
    })
  },
})

export const actions = { start }

export default servicesSlice.reducer
