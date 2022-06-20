import { createSlice } from '@reduxjs/toolkit'

import { WalletState } from './types'
import { unlockWallet, start, stop, updateWalletBalance } from './thunks'

export const initialState: WalletState = {
  running: false,
  pending: false,
  unlocked: false,
  address: '',
  tari: {
    balance: 0,
    available: 0,
    pending: true,
  },
}

const walletSlice = createSlice({
  name: 'wallet',
  initialState,
  reducers: {
    tariBalancePending(state) {
      state.tari.pending = true
    },
  },
  extraReducers: builder => {
    builder.addCase(unlockWallet.pending, state => {
      state.pending = true
    })
    builder.addCase(unlockWallet.fulfilled, (state, action) => {
      state.pending = false
      state.unlocked = true

      const {
        address,
        tari: { balance, available },
      } = action.payload
      state.address = address
      state.tari.balance = balance
      state.tari.available = available
    })

    builder.addCase(stop.pending, state => {
      state.pending = true
    })
    builder.addCase(stop.fulfilled, state => {
      state.pending = false
      state.running = false
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
}

export default walletSlice.reducer
