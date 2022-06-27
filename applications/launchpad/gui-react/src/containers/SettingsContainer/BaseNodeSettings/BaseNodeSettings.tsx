import { Controller, Control } from 'react-hook-form'

import Select from '../../../components/Select'
import Text from '../../../components/Text'
import SettingsSectionHeader from '../../../components/SettingsSectionHeader'

import t from '../../../locales'
import { Network } from '../../BaseNodeContainer/types'
import { networkOptions } from '../../BaseNodeContainer/constants'

import { SettingsInputs } from '../types'
import { useTheme } from 'styled-components'
import { Label } from '../../../components/Inputs/Input/styles'
import { InputRow, SelectRow } from './styles'
import Input from '../../../components/Inputs/Input'

const BaseNodeSettings = ({
  control,
  network,
}: {
  control: Control<SettingsInputs>
  network: Network
}) => {
  const theme = useTheme()
  return (
    <>
      <Text type='subheader' as='h2'>
        {t.baseNode.settings.title}
      </Text>
      <Controller
        name='baseNode.network'
        control={control}
        defaultValue={network}
        rules={{ required: true, minLength: 1 }}
        render={({ field }) => (
          <SelectRow>
            <Label>{t.baseNode.tari_network_label}</Label>
            <div style={{ width: '50%' }}>
              <Select
                value={networkOptions.find(
                  ({ value }) => value === field.value,
                )}
                options={networkOptions}
                onChange={({ value }) => field.onChange(value as Network)}
                fullWidth
              />
            </div>
          </SelectRow>
        )}
      />
      <SettingsSectionHeader noBottomMargin noTopMargin>
        {t.common.nouns.expert}
      </SettingsSectionHeader>
      <Controller
        name='baseNode.rootFolder'
        control={control}
        defaultValue={network}
        rules={{ required: true, minLength: 1 }}
        render={({ field }) => (
          <InputRow>
            <Label>{t.baseNode.settings.rootFolder}</Label>
            <Input
              onChange={field.onChange}
              value={field?.value?.toString() || ''}
              containerStyle={{ width: '75%' }}
              withError={false}
            />
          </InputRow>
        )}
      />
    </>
  )
}

export default BaseNodeSettings
