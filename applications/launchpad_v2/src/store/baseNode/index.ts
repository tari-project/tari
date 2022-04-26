import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import { BaseNodeState, Network } from './types'
import { startNode, stopNode } from './thunks'

const initialState: BaseNodeState = {
  network: 'mainnet',
  running: false,
  pending: false,
}

const baseNodeSlice = createSlice({
  name: 'app',
  initialState,
  reducers: {
    setTariNetwork(state, action: PayloadAction<Network>) {
      state.network = action.payload
    },
  },
  extraReducers: {
    [startNode.pending]: state => {
      state.pending = true
    },
    [startNode.fulfilled]: state => {
      state.running = true
      state.pending = false
    },
    [stopNode.pending]: state => {
      state.pending = true
    },
    [stopNode.fulfilled]: state => {
      state.running = false
      state.pending = false
    },
  },
})

const { setTariNetwork } = baseNodeSlice.actions
export const actions = {
  startNode,
  stopNode,
  setTariNetwork,
}

export default baseNodeSlice.reducer
