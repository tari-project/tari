import { createSlice, PayloadAction } from '@reduxjs/toolkit'
import { v4 as uuidv4 } from 'uuid'

import { startMiningNode, stopMiningNode } from './thunks'
import { MiningState } from './types'

const currencies: Record<'tari' | 'merged', string[]> = {
  tari: ['xtr'],
  merged: ['xtr', 'xmr'],
}

export const initialState: MiningState = {
  tari: {
    session: undefined,
  },
  merged: {
    threads: 1,
    address: undefined,
    urls: [],
    session: undefined,
  },
}

const miningSlice = createSlice({
  name: 'mining',
  initialState,
  reducers: {
    /**
     * @TODO - mock that need to be removed later. It is used along with timers in App.tsx
     * to increase the amount of mined Tari and Merged
     */
    addAmount(
      state,
      action: PayloadAction<{ amount: string; node: 'tari' | 'merged' }>,
    ) {
      const node = action.payload.node

      if (state[node].session?.total) {
        state[node].session!.total!.xtr = (
          Number(state[node].session!.total!.xtr) +
          Number(action.payload.amount)
        ).toString()
      }
    },
    startNewSession(state, action: PayloadAction<{ node: 'tari' | 'merged' }>) {
      const { node } = action.payload
      const total: Record<string, string> = {}
      currencies[node].forEach(c => {
        total[c] = '0'
      })

      state[node].session = {
        id: uuidv4(),
        startedAt: Number(Date.now()).toString(),
        total,
      }
    },
    stopSession(state, action: PayloadAction<{ node: 'tari' | 'merged' }>) {
      const { node } = action.payload

      const session = state[node].session
      if (session) {
        session.finishedAt = Number(Date.now()).toString()
      }
    },
    setMergedAddress(state, action: PayloadAction<{ address: string }>) {
      const { address } = action.payload
      state.merged.address = address
    },
  },
})

const { actions: miningActions } = miningSlice

export const actions = {
  ...miningActions,
  startMiningNode,
  stopMiningNode,
}

export default miningSlice.reducer
