import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import { CredentialsState } from './types'

export const initialState: CredentialsState = {}

const credentialsSlice = createSlice({
  name: 'credentials',
  initialState,
  reducers: {
    setWallet(state, { payload: wallet }: { payload: string }) {
      state.wallet = wallet
    },
    setMoneroCredentials(
      state,
      action: PayloadAction<{ username: string; password: string }>,
    ) {
      state.monero = action.payload
    },
  },
})

export const { actions } = credentialsSlice

export default credentialsSlice.reducer
