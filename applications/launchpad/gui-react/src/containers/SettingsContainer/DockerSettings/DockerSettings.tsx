import {
  Controller,
  Control,
  UseFormSetValue,
  FormState,
} from 'react-hook-form'
import { useTheme } from 'styled-components'

import Input from '../../../components/Inputs/Input'
import { Label } from '../../../components/Inputs/Input/styles'
import SettingsSectionHeader from '../../../components/SettingsSectionHeader'
import Text from '../../../components/Text'
import t from '../../../locales'
import { SettingsHeader } from '../styles'
import { SettingsInputs } from '../types'

import DockerImagesList from './DockerImagesList'

const DockerSettings = ({
  control,
}: {
  formState: FormState<SettingsInputs>
  control: Control<SettingsInputs>
  values: SettingsInputs
  setValue: UseFormSetValue<SettingsInputs>
  setOpenMiningAuthForm: (value: boolean) => void
}) => {
  const theme = useTheme()

  return (
    <>
      <SettingsHeader>
        <Text type='header' as='h1'>
          {t.docker.settings.title}
        </Text>
      </SettingsHeader>

      <SettingsSectionHeader noTopMargin>
        {t.common.nouns.expert}
      </SettingsSectionHeader>

      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'baseline',
          margin: `${theme.spacingVertical(0.5)} 0`,
        }}
      >
        <Label>Docker Tag</Label>
        <Controller
          name='docker.tag'
          control={control}
          rules={{ required: true, minLength: 1 }}
          render={({ field }) => (
            <Input
              onChange={field.onChange}
              value={field?.value?.toString() || ''}
              containerStyle={{ width: '50%' }}
              withError={false}
            />
          )}
        />
      </div>

      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'baseline',
          margin: `${theme.spacingVertical(0.5)} 0`,
        }}
      >
        <Label>Docker Registry</Label>
        <Controller
          name='docker.registry'
          control={control}
          rules={{ required: true, minLength: 1 }}
          render={({ field }) => (
            <Input
              onChange={field.onChange}
              value={field?.value?.toString() || ''}
              containerStyle={{ width: '50%' }}
              withError={false}
            />
          )}
        />
      </div>

      <SettingsSectionHeader>
        {t.docker.settings.imageStatuses}
      </SettingsSectionHeader>

      <DockerImagesList />
    </>
  )
}

export default DockerSettings
