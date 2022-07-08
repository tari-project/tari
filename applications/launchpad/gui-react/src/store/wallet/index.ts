import { createSlice } from '@reduxjs/toolkit'

import { WalletState } from './types'
import { unlockWallet, start, stop, updateWalletBalance } from './thunks'

export const initialState: WalletState = {
  unlocked: false,
  address: {
    uri: '',
    emoji: '',
  },
  tari: {
    balance: 0,
    available: 0,
    pending: true,
  },
  lastTxHistoryUpdateAt: undefined,
}

const walletSlice = createSlice({
  name: 'wallet',
  initialState,
  reducers: {
    tariBalancePending(state) {
      state.tari.pending = true
    },
    newTxInHistory(state) {
      state.lastTxHistoryUpdateAt = new Date()
    },
  },
  extraReducers: builder => {
    builder.addCase(unlockWallet.fulfilled, (state, action) => {
      state.unlocked = true

      const {
        address,
        tari: { balance, available },
      } = action.payload
      state.address = address
      state.tari.balance = balance
      state.tari.available = available
    })

    builder.addCase(updateWalletBalance.fulfilled, (state, { payload }) => {
      state.tari.balance = payload.tari.balance
      state.tari.available = payload.tari.available
      state.tari.pending = false
    })
    builder.addCase(updateWalletBalance.rejected, state => {
      state.tari.pending = false
    })
  },
})

export const actions = {
  unlockWallet,
  start,
  stop,
  updateWalletBalance,
  newTxInHistory: walletSlice.actions.newTxInHistory,
}

export default walletSlice.reducer
