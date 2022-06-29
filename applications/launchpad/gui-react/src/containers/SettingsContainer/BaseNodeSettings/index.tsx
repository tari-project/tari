import BaseNodeSettings from './BaseNodeSettings'
import { Control } from 'react-hook-form'
import { SettingsInputs } from '../types'

const BaseNodeSettingsContainer = ({
  control,
  onBaseNodeConnectClick,
}: {
  control: Control<SettingsInputs>
  onBaseNodeConnectClick: () => void
}) => {
  return (
    <BaseNodeSettings
      control={control}
      onBaseNodeConnectClick={onBaseNodeConnectClick}
    />
  )
}

export default BaseNodeSettingsContainer
