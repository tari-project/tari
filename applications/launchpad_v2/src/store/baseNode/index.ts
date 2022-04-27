import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import { BaseNodeState } from './types'
import { startNode, stopNode } from './thunks'
import { Network } from '../../containers/BaseNodeContainer/types'

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
    [`${startNode.pending}`]: (state: BaseNodeState) => {
      state.pending = true
    },
    [`${startNode.fulfilled}`]: (state: BaseNodeState) => {
      state.running = true
      state.pending = false
    },
    [`${stopNode.pending}`]: (state: BaseNodeState) => {
      state.pending = true
    },
    [`${stopNode.fulfilled}`]: (state: BaseNodeState) => {
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
