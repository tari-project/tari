import { createAsyncThunk, createSlice, PayloadAction } from '@reduxjs/toolkit'

import { BaseNodeState, Network } from '../types'

const initialState: BaseNodeState = {
  network: 'mainnet',
  running: false,
  pending: false,
}

const startNode = createAsyncThunk('startNode', async (payload, thunkAPI) => {
  const {
    baseNode: { network },
  } = thunkAPI.getState()

  console.log(`starting base node on network ${network}`)
  await new Promise(resolve => setTimeout(resolve, 2000))
})

const stopNode = createAsyncThunk('stopNode', async (payload, thunkAPI) => {
  const {
    baseNode: { network },
  } = thunkAPI.getState()

  console.log(`stopping base node on network ${network}`)
  await new Promise(resolve => setTimeout(resolve, 2000))
})

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
