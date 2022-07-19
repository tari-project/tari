import { createSlice, PayloadAction } from '@reduxjs/toolkit'

import { SettingsInputs } from '../../containers/SettingsContainer/types'

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
    save(state, action: PayloadAction<Pick<SettingsInputs, 'docker'>>) {
      state.serviceSettings = {
        ...state.serviceSettings,
        dockerTag: action.payload.docker.tag,
        dockerRegistry: action.payload.docker.registry,
      }
    },
  },
  extraReducers: builder => {
    builder.addCase(loadDefaultServiceSettings.fulfilled, (state, action) => {
      const settings = action.payload
      state.serviceSettings = {
        dockerRegistry: settings.dockerRegistry,
        dockerTag: settings.dockerTag,
      }
    })
  },
})

const { actions: syncActions } = settingsSlice

export const actions = {
  ...syncActions,
  loadDefaultServiceSettings,
}

export default settingsSlice.reducer
