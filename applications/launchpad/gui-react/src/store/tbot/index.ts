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
        /**
         * @TODO - not sure if we can use this filtering, because tbot keeps whole history.
         * So if the use hits the same 'help' after a while, he won't see new messages, bc they are already in queque
         * prevent duplicate messages
         */
        // if (!state.messageQueue.includes(str)) {
        state.messageQueue = [...state.messageQueue, str]
        // }
      })
      state.open = true
    },
    close(state) {
      state.open = false
    },
  },
})

const { actions: syncActions } = tbotSlice

export const tbotactions = {
  ...syncActions,
}

export default tbotSlice.reducer
