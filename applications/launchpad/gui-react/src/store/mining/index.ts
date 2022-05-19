import { createSlice, PayloadAction } from '@reduxjs/toolkit'
import { v4 as uuidv4 } from 'uuid'

import { startMiningNode, stopMiningNode } from './thunks'
import { MiningSession, MiningState } from './types'

const currencies: Record<'tari' | 'merged', string[]> = {
  tari: ['xtr'],
  merged: ['xtr', 'xmr'],
}

export const initialState: MiningState = {
  tari: {
    sessions: [
      {
        startedAt: undefined,
        finishedAt: Number(Date.now()).toString(),
        pending: false,
        total: {
          xtr: '1000',
        },
      },
      {
        startedAt: undefined,
        finishedAt: Number(Date.now()).toString(),
        pending: false,
        total: {
          xtr: '2000',
        },
      },
    ],
  },
  merged: {
    threads: 1,
    address: undefined,
    urls: [],
    sessions: [
      {
        startedAt: undefined,
        finishedAt: Number(Date.now()).toString(),
        pending: false,
        total: {
          xtr: '1000',
          xmr: '1001',
        },
      },
      {
        startedAt: undefined,
        finishedAt: Number(Date.now()).toString(),
        pending: false,
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
    /**
     * @TODO - mock that need to be removed later. It is used along with timers in App.tsx
     * to increase the amount of mined Tari and Merged
     */
    addAmount(
      state,
      action: PayloadAction<{ amount: string; node: 'tari' | 'merged' }>,
    ) {
      const node = action.payload.node

      state[node].sessions![state[node].sessions!.length - 1].total!.xtr = (
        Number(
          state[node].sessions![state[node].sessions!.length - 1].total!.xtr,
        ) + Number(action.payload.amount)
      ).toString()
    },
    startNewSession(state, action: PayloadAction<{ node: 'tari' | 'merged' }>) {
      const { node } = action.payload
      const total: Record<string, string> = {}
      currencies[node].forEach(c => {
        total[c] = '0'
      })

      const newSession: MiningSession = {
        id: uuidv4(),
        startedAt: Number(Date.now()).toString(),
        total,
      }

      if (!state[node].sessions) {
        state[node].sessions = [newSession]
        return
      }

      state[node].sessions!.push(newSession)
    },
    stopSession(
      state,
      action: PayloadAction<{ node: 'tari' | 'merged'; sessionId?: string }>,
    ) {
      const { node, sessionId } = action.payload

      if (!state[node].sessions || !sessionId) {
        return
      }

      const session = state[node].sessions?.find(s => s.id === sessionId)

      if (session) {
        session.finishedAt = Number(Date.now()).toString()
      }
    },
    setPendingInSession(
      state,
      action: PayloadAction<{
        node: 'tari' | 'merged'
        sessionId?: string
        active: boolean
      }>,
    ) {
      const { node, sessionId, active } = action.payload

      if (!state[node].sessions || !sessionId) {
        return
      }

      const session = state[node].sessions?.find(s => s.id === sessionId)

      if (session) {
        session.pending = active
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
