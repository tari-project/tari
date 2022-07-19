import { Control, useFieldArray } from 'react-hook-form'

import Button from '../../../../components/Button'

import MoneroURL from '../MoneroURL'
import { SettingsInputs } from '../../types'

import MiningConfig from '../../../../config/mining'
import t from '../../../../locales'
import { ActionsContainer, UrlList } from '../styles'

const MoneroURLs = ({ control }: { control: Control<SettingsInputs> }) => {
  const { fields, append, remove } = useFieldArray({
    control,
    name: 'mining.merged.urls',
  })

  return (
    <UrlList>
      {fields.map((field, index) => (
        <MoneroURL
          key={field.id}
          url={field.url}
          control={control}
          remove={remove}
          index={index}
        />
      ))}
      {fields.length < MiningConfig.maxMoneroUrls && (
        <ActionsContainer>
          <Button
            variant='button-in-text'
            onClick={() => append({ url: '' })}
            testId='add-new-monero-url-btn'
          >
            {t.mining.settings.addNextUrl}
          </Button>
        </ActionsContainer>
      )}
    </UrlList>
  )
}

export default MoneroURLs
