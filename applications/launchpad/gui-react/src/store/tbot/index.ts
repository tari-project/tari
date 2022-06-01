import { createSlice } from '@reduxjs/toolkit'

import { TBotState } from './types'

export const initialState: TBotState = {
  messageQueue: [],
  open: false,
}

const tbotSlice = createSlice({
  name: 'tbot',
  initialState,
  reducers: {
    push(state, action) {
      action.payload.map((str: string) => {
        if (!state.messageQueue.includes(str)) {
          state.messageQueue = [...state.messageQueue, str]
        }
      })
      state.open = true
    },
    close(state) {
      state.messageQueue = []
      state.open = false
    },
  },
})

const { actions: syncActions } = tbotSlice

export const tbotactions = {
  ...syncActions,
}

export default tbotSlice.reducer
