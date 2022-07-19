import { createSlice } from '@reduxjs/toolkit'

import { TemporaryState } from './types'

export const initialState: TemporaryState = {
  walletPasswordConfirmation: undefined,
}

const temporarySlice = createSlice({
  name: 'temporary',
  initialState,
  reducers: {
    setWalletPasswordConfirmation(state, action) {
      state.walletPasswordConfirmation = action.payload
    },
  },
})

const { actions: temporarySliceActions } = temporarySlice

export const temporaryActions = {
  ...temporarySliceActions,
}

export default temporarySlice.reducer
