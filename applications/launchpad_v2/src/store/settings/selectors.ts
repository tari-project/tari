import { RootState } from '../'

export const selectSettingsOpen = (state: RootState) => state.settings.open
export const selectActiveSettings = (state: RootState) => state.settings.which
