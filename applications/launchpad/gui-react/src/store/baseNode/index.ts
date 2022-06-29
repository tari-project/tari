import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import { startNode, stopNode } from './thunks'
import { Network } from '../../containers/BaseNodeContainer/types'

const initialState = {
  network: 'dibbler',
  rootFolder: '',
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
})

const { setTariNetwork, setRootFolder } = baseNodeSlice.actions
export const actions = {
  startNode,
  stopNode,
  setTariNetwork,
  setRootFolder,
}

export default baseNodeSlice.reducer
