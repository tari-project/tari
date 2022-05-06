import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import { startNode, stopNode } from './thunks'
import { Network } from '../../containers/BaseNodeContainer/types'

const initialState = {
  network: 'dibbler',
}

const baseNodeSlice = createSlice({
  name: 'baseNode',
  initialState,
  reducers: {
    setTariNetwork(state, action: PayloadAction<Network>) {
      state.network = action.payload
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
