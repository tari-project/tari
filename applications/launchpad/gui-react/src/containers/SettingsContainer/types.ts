import { Settings } from '../../store/settings/types'

export type SettingsProps = {
  onSettingsTouched: (changed: boolean) => void
}

export type SettingsComponentProps = {
  open?: boolean
  onClose: () => void
  goToSettings: (s: Settings) => void
  activeSettings: Settings
}
