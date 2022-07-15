import { createSlice, PayloadAction } from '@reduxjs/toolkit'
import { v4 as uuidv4 } from 'uuid'

import { MiningNodeType, ScheduleId } from '../../types/general'
import MiningConfig from '../../config/mining'

import {
  startMiningNode,
  stopMiningNode,
  notifyUserAboutMinedTariBlock,
  addMinedTx,
} from './thunks'
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
    useAuth: false,
    session: undefined,
  },
  notifications: [],
}

const miningSlice = createSlice({
  name: 'mining',
  initialState,
  reducers: {
    startNewSession(
      state,
      action: PayloadAction<{
        node: MiningNodeType
        reason: MiningActionReason
        schedule?: ScheduleId
      }>,
    ) {
      const { node, reason, schedule } = action.payload
      const total: Record<string, number> = {}
      currencies[node].forEach(c => {
        total[c] = 0
      })

      state[node].session = {
        id: uuidv4(),
        startedAt: Number(Date.now()).toString(),
        total,
        reason,
        schedule,
        history: [],
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
        authentication?: {
          username?: string
          password?: string
        }
        useAuth?: boolean
      }>,
    ) {
      const safePayload = { ...action.payload }
      delete safePayload['authentication']

      state.merged = { ...state.merged, ...safePayload }
    },
    acknowledgeNotification(state) {
      const [_head, ...notificationsLeft] = state.notifications
      state.notifications = notificationsLeft
    },
  },
  extraReducers: builder => {
    builder.addCase(
      notifyUserAboutMinedTariBlock.fulfilled,
      (state, action) => {
        const newNotification = {
          ...action.meta.arg,
          ...action.payload,
        }

        state.notifications = [...state.notifications, newNotification]
      },
    )

    builder.addCase(addMinedTx.fulfilled, (state, action) => {
      const node = action.payload.node
      const session = state[node].session

      if (!session) {
        return
      }

      const nodeSessionTotal = state[node].session?.total?.xtr || 0

      let total = session.total
      if (!total) {
        total = { xtr: 0 }
      }

      total.xtr = nodeSessionTotal + action.payload.amount

      session.history.push({
        txId: action.payload.txId,
        amount: action.payload.amount,
      })
    })
  },
})

const { actions: miningActions } = miningSlice

export const actions = {
  ...miningActions,
  startMiningNode,
  stopMiningNode,
  notifyUserAboutMinedTariBlock,
  addMinedTx,
}

export default miningSlice.reducer
