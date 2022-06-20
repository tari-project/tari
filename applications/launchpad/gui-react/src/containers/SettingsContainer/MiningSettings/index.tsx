import MiningSettings from './MiningSettings'
import { Control, UseFormSetValue } from 'react-hook-form'
import { SettingsInputs } from '../types'

const MiningSettingsContainer = ({
  control,
  values,
  setValue,
  setOpenMiningAuthForm,
}: {
  control: Control<SettingsInputs>
  values: SettingsInputs
  setValue: UseFormSetValue<SettingsInputs>
  setOpenMiningAuthForm: (value: boolean) => void
}) => {
  return (
    <MiningSettings
      control={control}
      values={values}
      setValue={setValue}
      setOpenMiningAuthForm={setOpenMiningAuthForm}
    />
  )
}

export default MiningSettingsContainer
