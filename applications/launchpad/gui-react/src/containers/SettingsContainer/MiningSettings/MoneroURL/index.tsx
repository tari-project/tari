import { Control, Controller, UseFieldArrayRemove } from 'react-hook-form'
import Button from '../../../../components/Button'
import Input from '../../../../components/Inputs/Input'
import { SettingsInputs } from '../../types'
import { HeaderRow, StyledMoneroURL } from './styles'

import t from '../../../../locales'
import { Label } from '../../../../components/Inputs/Input/styles'
import SvgTrash2 from '../../../../styles/Icons/Trash2'
import { useTheme } from 'styled-components'
import { isUrl } from '../../../../utils/Validators'

const MoneroURL = ({
  url,
  control,
  remove,
  index,
}: {
  url: string
  username?: string
  password?: string
  control: Control<SettingsInputs>
  remove: UseFieldArrayRemove
  index: number
}) => {
  const theme = useTheme()

  const urlFormat = (value: string) => {
    return isUrl(value) ? undefined : t.mining.settings.wrongUrlFormat
  }

  return (
    <StyledMoneroURL>
      <HeaderRow>
        <Label>{t.mining.settings.moneroUrlLabel}</Label>
        <div className='header-buttons'>
          <Button
            variant='button-in-text'
            rightIcon={<SvgTrash2 color={theme.warningDark} />}
            onClick={() => remove(index)}
            testId={`mining-url-remove-${index}`}
          />
        </div>
      </HeaderRow>
      <Controller
        name={`mining.merged.urls.${index}.url`}
        control={control}
        defaultValue={url}
        rules={{
          required: true,
          minLength: 1,
          validate: { urlFormat },
        }}
        render={({ field, fieldState }) => (
          <Input
            placeholder={t.mining.settings.moneroUrlPlaceholder}
            testId={`mining-url-input-${index}`}
            onChange={value => field.onChange(value)}
            value={field.value}
            containerStyle={{ width: '100%' }}
            error={fieldState.error ? fieldState.error.message : ''}
          />
        )}
      />
    </StyledMoneroURL>
  )
}

export default MoneroURL
