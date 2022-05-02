import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import { SettingsState, Settings } from './types'

export const initialState: SettingsState = {
  open: false,
  which: Settings.Mining,
}

const settingsSlice = createSlice({
  name: 'settings',
  initialState,
  reducers: {
    close(state) {
      state.open = false
      state.which = Settings.Mining
    },
    open(state) {
      state.open = true
    },
    goTo(state, action: PayloadAction<Settings>) {
      state.which = action.payload
    },
  },
})

export const { actions } = settingsSlice

export default settingsSlice.reducer
