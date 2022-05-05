import { sep } from '@tauri-apps/api/path'

import { RootState } from '../'

export const selectSettingsOpen = (state: RootState) => state.settings.open
export const selectActiveSettings = (state: RootState) => state.settings.which
export const selectServiceSettings = (state: RootState) => ({
  ...state.settings.serviceSettings,
  tariNetwork: state.baseNode.network,
  rootFolder:
    state.settings.serviceSettings.cacheDir +
    'tari' +
    sep +
    'tmp' +
    sep +
    state.baseNode.network,
})
