import { createSlice, PayloadAction } from '@reduxjs/toolkit'
import { v4 as uuidv4 } from 'uuid'
import { MiningNodeType, ScheduleId } from '../../types/general'
import MiningConfig from '../../config/mining'

import { startMiningNode, stopMiningNode } from './thunks'
import { MiningState, MiningActionReason, MoneroUrl } from './types'

const currencies: Record<MiningNodeType, string[]> = {
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
    urls: MiningConfig.defaultMoneroUrls.map(url => ({ url })),
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
      action: PayloadAction<{ amount: string; node: MiningNodeType }>,
    ) {
      const node = action.payload.node

      if (state[node].session?.total) {
        const nodeSessionTotal = state[node].session?.total?.xtr
        // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
        state[node].session!.total!.xtr = (
          Number(nodeSessionTotal) + Number(action.payload.amount)
        ).toString()
      }
    },
    startNewSession(
      state,
      action: PayloadAction<{
        node: MiningNodeType
        reason: MiningActionReason
        schedule?: ScheduleId
      }>,
    ) {
      const { node, reason, schedule } = action.payload
      const total: Record<string, string> = {}
      currencies[node].forEach(c => {
        total[c] = '0'
      })

      state[node].session = {
        id: uuidv4(),
        startedAt: Number(Date.now()).toString(),
        total,
        reason,
        schedule,
      }
    },
    stopSession(
      state,
      action: PayloadAction<{
        node: MiningNodeType
        reason: MiningActionReason
      }>,
    ) {
      const { node, reason } = action.payload

      const session = state[node].session
      if (session) {
        session.finishedAt = Number(Date.now()).toString()
        session.reason = reason
      }
    },
    setMergedAddress(state, action: PayloadAction<{ address: string }>) {
      const { address } = action.payload
      state.merged.address = address
    },
    setMergedConfig(
      state,
      action: PayloadAction<{
        address?: string
        threads?: number
        urls?: MoneroUrl[]
      }>,
    ) {
      state.merged = { ...state.merged, ...action.payload }
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
