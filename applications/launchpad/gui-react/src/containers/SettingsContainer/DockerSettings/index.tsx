import { Control, FormState, UseFormSetValue } from 'react-hook-form'

import { SettingsInputs } from '../types'

import DockerSettings from './DockerSettings'

const DockerSettingsContainer = ({
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
    <DockerSettings
      formState={formState}
      control={control}
      values={values}
      setValue={setValue}
      setOpenMiningAuthForm={setOpenMiningAuthForm}
    />
  )
}

export default DockerSettingsContainer
