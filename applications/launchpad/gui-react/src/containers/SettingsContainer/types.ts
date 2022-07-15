import { Control, FormState, UseFormSetValue } from 'react-hook-form'
import { MergedMiningNodeState, MoneroUrl } from '../../store/mining/types'
import { Settings } from '../../store/settings/types'
import { ThemeType } from '../../styles/themes/types'
import { Network } from '../BaseNodeContainer/types'

export type SettingsProps = {
  formState: FormState<SettingsInputs>
  control: Control<SettingsInputs>
  values: SettingsInputs
  setValue: UseFormSetValue<SettingsInputs>
  setOpenMiningAuthForm: (value: boolean) => void
  onBaseNodeConnectClick: () => void
}

export interface SettingsInputs {
  mining: {
    merged: MiningSettingsInputs
  }
  docker: {
    tag: string
    registry: string
  }
  baseNode: BaseNodeSettingsInputs
}

export interface AuthenticationInputs {
  username: string
  password: string
}

export interface MiningSettingsInputs {
  address: string
  threads: number
  urls: MoneroUrl[]
  authentication?: AuthenticationInputs
  useAuth: boolean
}

export interface BaseNodeSettingsInputs {
  network: Network
  rootFolder: string
}

export type SettingsComponentProps = {
  open?: boolean
  onClose: () => void
  goToSettings: (s: Settings) => void
  activeSettings: Settings
  formState: FormState<SettingsInputs>
  defaultMiningMergedValues?: MergedMiningNodeState
  values: SettingsInputs
  setValue: UseFormSetValue<SettingsInputs>
  onSubmit: () => void
  control: Control<SettingsInputs>
  confirmCancel: boolean
  cancelDiscard: () => void
  discardChanges: () => void
  openMiningAuthForm: boolean
  setOpenMiningAuthForm: (value: boolean) => void
  openBaseNodeConnect: boolean
  setOpenBaseNodeConnect: (value: boolean) => void
  currentTheme: ThemeType
  changeTheme: (theme: ThemeType) => void
}
