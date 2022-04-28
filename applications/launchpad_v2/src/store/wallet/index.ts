import { createSlice } from '@reduxjs/toolkit'

import { WalletState } from './types'
import { unlockWallet } from './thunks'

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
      state.running = true
      const {
        address,
        tari: { balance, available },
      } = action.payload
      state.address = address
      state.tari.balance = balance
      state.tari.available = available
    })
  },
})

export const actions = {
  unlockWallet,
}

export default walletSlice.reducer
