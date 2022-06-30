import BaseNodeSettings from './BaseNodeSettings'
import { Control, UseFormSetValue } from 'react-hook-form'
import { SettingsInputs } from '../types'

const BaseNodeSettingsContainer = ({
  control,
  onBaseNodeConnectClick,
  setValue,
}: {
  control: Control<SettingsInputs>
  onBaseNodeConnectClick: () => void
  setValue: UseFormSetValue<SettingsInputs>
}) => {
  return (
    <BaseNodeSettings
      control={control}
      onBaseNodeConnectClick={onBaseNodeConnectClick}
      setValue={setValue}
    />
  )
}

export default BaseNodeSettingsContainer
