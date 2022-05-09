import { createSlice } from '@reduxjs/toolkit'
import { MiningNodeType } from '../../types/general'
import { startMiningNode, stopMiningNode } from './thunks'

import { MiningNodesStatus, MiningState } from './types'

export const initialState: MiningState = {
  tari: {
    pending: false,
    status: MiningNodesStatus.SETUP_REQUIRED,
    sessions: [
      {
        total: {
          xtr: '1000',
        },
      },
      {
        total: {
          xtr: '2000',
        },
      },
    ],
  },
  merged: {
    pending: false,
    status: MiningNodesStatus.PAUSED,
    sessions: [
      {
        total: {
          xtr: '1000',
          xmr: '1001',
        },
      },
      {
        total: {
          xtr: '2000',
          xmr: '2001',
        },
      },
    ],
  },
}

const miningSlice = createSlice({
  name: 'mining',
  initialState,
  reducers: {
    setNodeStatus(
      state,
      {
        payload,
      }: { payload: { node: MiningNodeType; status: MiningNodesStatus } },
    ) {
      state[payload.node].status = payload.status
    },
  },
  extraReducers: builder => {
    builder
      .addCase(startMiningNode.pending, (state, action) => {
        const node = action.meta.arg.node
        if (node in state) {
          state[node].pending = true
        }
      })
      .addCase(startMiningNode.fulfilled, (state, action) => {
        const node = action.meta.arg.node
        if (node in state) {
          state[node].pending = false
          state[node].status = MiningNodesStatus.RUNNING
        }
      })
      .addCase(stopMiningNode.pending, (state, action) => {
        const node = action.meta.arg.node
        if (node in state) {
          state[node].pending = true
        }
      })
      .addCase(stopMiningNode.fulfilled, (state, action) => {
        const node = action.meta.arg.node
        if (node in state) {
          state[node].pending = false
          state[node].status = MiningNodesStatus.PAUSED
        }
      })
  },
})

const { setNodeStatus } = miningSlice.actions
export const actions = {
  setNodeStatus,
  startMiningNode,
  stopMiningNode,
}

export default miningSlice.reducer
