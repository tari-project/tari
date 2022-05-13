import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import { TBotState } from './types'

export const initialState: TBotState = {
  // open: true,
  messageQueue: [],
}

const tbotSlice = createSlice({
  name: 'tbot',
  initialState,
  reducers: {
    push(state, action) {
      state.messageQueue = [...state.messageQueue, action.payload]
    },
    close(state) {
      state.messageQueue = []
    },
  },
})

const { actions: syncActions } = tbotSlice

export const tbotactions = {
  ...syncActions,
}

export default tbotSlice.reducer
