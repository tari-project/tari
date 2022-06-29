import { useAppSelector } from '../../../store/hooks'

import BaseNodeSettings from './BaseNodeSettings'
import { selectState as selectBaseNodeState } from '../../../store/baseNode/selectors'
import { Control } from 'react-hook-form'
import { SettingsInputs } from '../types'

const BaseNodeSettingsContainer = ({
  control,
  onBaseNodeConnectClick,
}: {
  control: Control<SettingsInputs>
  onBaseNodeConnectClick: () => void
}) => {
  const { network } = useAppSelector(selectBaseNodeState)

  return (
    <BaseNodeSettings
      control={control}
      network={network}
      onBaseNodeConnectClick={onBaseNodeConnectClick}
    />
  )
}

export default BaseNodeSettingsContainer
