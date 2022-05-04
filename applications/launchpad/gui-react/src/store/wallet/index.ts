import { createSlice } from '@reduxjs/toolkit'

import { WalletState } from './types'
import { unlockWallet, start, stop } from './thunks'

export const initialState: WalletState = {
  running: false,
  pending: false,
  unlocked: false,
  address: '',
  tari: {
    balance: 0,
    available: 0,
  },
}

const walletSlice = createSlice({
  name: 'wallet',
  initialState,
  reducers: {},
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

    builder.addCase(start.pending, state => {
      state.pending = true
    })
    builder.addCase(start.fulfilled, state => {
      state.pending = false
      state.running = true
    })

    builder.addCase(stop.pending, state => {
      state.pending = true
    })
    builder.addCase(stop.fulfilled, state => {
      state.pending = false
      state.running = false
    })
  },
})

export const actions = {
  unlockWallet,
  start,
  stop,
}

export default walletSlice.reducer
