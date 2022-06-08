import { Control, FormState } from 'react-hook-form'
import { MoneroUrl } from '../../store/mining/types'
import { Settings } from '../../store/settings/types'
import { Network } from '../BaseNodeContainer/types'

export type SettingsProps = {
  control: Control<SettingsInputs>
}

export interface SettingsInputs {
  mining: {
    merged: MiningSettingsInputs
  }
  baseNode: BaseNodeSettingsInputs
}

export interface MiningSettingsInputs {
  address: string
  threads: number
  urls: MoneroUrl[]
}

export interface BaseNodeSettingsInputs {
  network: Network
}

export type SettingsComponentProps = {
  open?: boolean
  onClose: () => void
  goToSettings: (s: Settings) => void
  activeSettings: Settings
  formState: FormState<SettingsInputs>
  onSubmit: () => void
  control: Control<SettingsInputs>
  confirmCancel: boolean
  cancelDiscard: () => void
  discardChanges: () => void
}
