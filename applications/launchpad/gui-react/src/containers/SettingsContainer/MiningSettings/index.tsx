import MiningSettings from './MiningSettings'
import { Control, FormState, UseFormSetValue } from 'react-hook-form'
import { SettingsInputs } from '../types'

const MiningSettingsContainer = ({
  formState,
  control,
  values,
  setValue,
  setOpenMiningAuthForm,
}: {
  formState: FormState<SettingsInputs>
  control: Control<SettingsInputs>
  values: SettingsInputs
  setValue: UseFormSetValue<SettingsInputs>
  setOpenMiningAuthForm: (value: boolean) => void
}) => {
  return (
    <MiningSettings
      formState={formState}
      control={control}
      values={values}
      setValue={setValue}
      setOpenMiningAuthForm={setOpenMiningAuthForm}
    />
  )
}

export default MiningSettingsContainer
