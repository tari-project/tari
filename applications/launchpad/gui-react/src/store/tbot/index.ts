import { createSlice } from '@reduxjs/toolkit'

import { TBotState } from './types'

export const initialState: TBotState = {
  messageQueue: [],
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
