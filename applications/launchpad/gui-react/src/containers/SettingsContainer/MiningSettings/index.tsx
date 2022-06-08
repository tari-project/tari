import MiningSettings from './MiningSettings'
import { Control } from 'react-hook-form'
import { SettingsInputs } from '../types'

const MiningSettingsContainer = ({
  control,
}: {
  control: Control<SettingsInputs>
}) => {
  return <MiningSettings control={control} />
}

export default MiningSettingsContainer
