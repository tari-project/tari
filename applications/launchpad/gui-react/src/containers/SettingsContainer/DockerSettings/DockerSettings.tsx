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
import DockerImagesList from '../../../components/DockerImagesList'
import t from '../../../locales'
import { SettingsHeader } from '../styles'
import { SettingsInputs } from '../types'

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
        <Text type='subheader' as='h2' color={theme.primary}>
          {t.docker.settings.title}
        </Text>
      </SettingsHeader>

      <SettingsSectionHeader noTopMargin noBottomMargin>
        {t.common.nouns.expert}
      </SettingsSectionHeader>

      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          margin: `${theme.spacingVertical(0.5)} 0`,
        }}
      >
        <Label $noMargin>{t.docker.settings.tagLabel}</Label>
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
          alignItems: 'center',
          margin: `${theme.spacingVertical(0.5)} 0`,
        }}
      >
        <Label $noMargin>{t.docker.settings.registryLabel}</Label>
        <Controller
          name='docker.registry'
          control={control}
          rules={{ required: true, minLength: 1 }}
          render={({ field }) => (
            <Input
              onChange={field.onChange}
              value={field?.value?.toString() || ''}
              containerStyle={{
                width: '50%',
              }}
              withError={false}
            />
          )}
        />
      </div>

      <SettingsSectionHeader noTopMargin>
        {t.docker.settings.imageStatuses}
      </SettingsSectionHeader>

      <DockerImagesList
        style={{
          marginBottom: theme.spacing(),
          marginLeft: `-${theme.spacingHorizontal(1.2)}`,
        }}
      />
    </>
  )
}

export default DockerSettings
