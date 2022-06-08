import { Controller, Control } from 'react-hook-form'

import Select from '../../../components/Select'
import Text from '../../../components/Text'

import t from '../../../locales'
import { Network } from '../../BaseNodeContainer/types'
import { networkOptions } from '../../BaseNodeContainer/constants'

import { SettingsInputs } from '../types'

const BaseNodeSettings = ({
  control,
  network,
}: {
  control: Control<SettingsInputs>
  network: Network
}) => {
  return (
    <>
      <Text type='header'>{t.baseNode.settings.title}</Text>

      <Controller
        name='baseNode.network'
        control={control}
        defaultValue={network}
        rules={{ required: true, minLength: 1 }}
        render={({ field }) => (
          <Select
            value={networkOptions.find(({ value }) => value === field.value)}
            options={networkOptions}
            onChange={({ value }) => field.onChange(value as Network)}
            label={t.baseNode.tari_network_label}
          />
        )}
      />
    </>
  )
}

export default BaseNodeSettings
