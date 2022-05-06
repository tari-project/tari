import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import { SettingsState, Settings } from './types'
import { loadDefaultServiceSettings } from './thunks'

export const initialState: SettingsState = {
  open: false,
  which: Settings.Mining,
  serviceSettings: {},
}

const settingsSlice = createSlice({
  name: 'settings',
  initialState,
  reducers: {
    close(state) {
      state.open = false
      state.which = Settings.Mining
    },
    open(state, action: PayloadAction<{ toOpen?: Settings }>) {
      state.open = true
      if (action.payload.toOpen) {
        state.which = action.payload.toOpen
      }
    },
    goTo(state, action: PayloadAction<Settings>) {
      state.which = action.payload
    },
  },
  extraReducers: builder => {
    builder.addCase(loadDefaultServiceSettings.fulfilled, (state, action) => {
      state.serviceSettings = action.payload
    })
  },
})

const { actions: syncActions } = settingsSlice

export const actions = {
  ...syncActions,
  loadDefaultServiceSettings,
}

export default settingsSlice.reducer
