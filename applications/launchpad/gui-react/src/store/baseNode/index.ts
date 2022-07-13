import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import { getBaseNodeIdentity, startNode, stopNode } from './thunks'
import { Network } from '../../containers/BaseNodeContainer/types'
import { BaseNodeState } from './types'

const initialState: BaseNodeState = {
  network: 'dibbler',
  rootFolder: '',
  identity: undefined,
}

const baseNodeSlice = createSlice({
  name: 'baseNode',
  initialState,
  reducers: {
    setTariNetwork(state, action: PayloadAction<Network>) {
      state.network = action.payload
    },
    setRootFolder(state, action: PayloadAction<string>) {
      state.rootFolder = action.payload
    },
  },
  extraReducers: builder => {
    builder.addCase(getBaseNodeIdentity.fulfilled, (state, action) => {
      state.identity = action.payload
    })
  },
})

const { setTariNetwork, setRootFolder } = baseNodeSlice.actions
export const actions = {
  getBaseNodeIdentity,
  startNode,
  stopNode,
  setTariNetwork,
  setRootFolder,
}

export default baseNodeSlice.reducer
