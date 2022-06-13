import { Controller, Control } from 'react-hook-form'

import Input from '../../../components/Inputs/Input'
import { Label } from '../../../components/Inputs/Input/styles'
import SettingsSectionHeader from '../../../components/SettingsSectionHeader'
import Text from '../../../components/Text'
import MiningConfig from '../../../config/mining'

import t from '../../../locales'
import { SettingsHeader } from '../styles'
import { SettingsInputs } from '../types'
import MoneroURLs from './MoneroURLs'
import { AddressDescription, NarrowInlineInput } from './styles'

const MiningSettings = ({ control }: { control: Control<SettingsInputs> }) => {
  return (
    <>
      <SettingsHeader>
        <Text type='header' as='h1'>
          {t.mining.settings.title}
        </Text>
      </SettingsHeader>

      <div style={{ width: '70%' }}>
        <Controller
          name='mining.merged.address'
          control={control}
          rules={{ required: true, minLength: 1 }}
          render={({ field }) => (
            <Input
              placeholder={t.mining.setup.addressPlaceholder}
              label={t.mining.settings.moneroAddressLabel}
              testId='address-input'
              value={field.value?.toString()}
              onChange={v => field.onChange(v)}
              autoFocus
            />
          )}
        />
      </div>

      <AddressDescription>
        <Text type='smallMedium'>
          {t.mining.settings.moneroAddressDesc1}
          <br />
          {t.mining.settings.moneroAddressDesc2}
        </Text>
      </AddressDescription>

      <SettingsSectionHeader>{t.common.nouns.expert}</SettingsSectionHeader>

      <NarrowInlineInput>
        <Label>{t.mining.settings.threadsLabel}</Label>
        <Controller
          name='mining.merged.threads'
          control={control}
          rules={{ required: true, minLength: 1 }}
          render={({ field }) => (
            <Input
              testId='mining-merged-threads-input'
              onChange={value => {
                // convert string into number
                const stripped = value.replace(/\D/g, '')
                let val = !stripped
                  ? ''
                  : Math.abs(Math.round(parseInt(stripped)))

                // limit the number of threads to maxThreads
                if (val > MiningConfig.maxThreads) {
                  val = MiningConfig.maxThreads
                }
                field.onChange(val)
              }}
              value={field?.value?.toString() || ''}
              containerStyle={{ maxWidth: 96 }}
            />
          )}
        />
      </NarrowInlineInput>

      <div>
        <MoneroURLs control={control} />
      </div>
    </>
  )
}

export default MiningSettings
