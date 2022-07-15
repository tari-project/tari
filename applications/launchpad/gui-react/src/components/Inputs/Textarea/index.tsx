import { ChangeEvent } from 'react'
import { Label } from '../Input/styles'
import Text from '../../Text'
import { InputContainer, StyledTextarea, ErrorContainer } from './styles'
import { TextareaProps } from './types'
import { useTheme } from 'styled-components'

const Textarea = ({
  id,
  value,
  rows,
  cols,
  label,
  placeholder,
  style,
  onChange,
  disabled,
  withError,
  error,
  inverted,
  testId,
}: TextareaProps) => {
  const theme = useTheme()

  const onChangeTextLocal = (event: ChangeEvent<HTMLTextAreaElement>) => {
    if (onChange) {
      onChange(event.target.value)
    }
  }

  return (
    <>
      {label && (
        <Label htmlFor={id} $inverted={inverted}>
          {label}
        </Label>
      )}
      <InputContainer>
        <StyledTextarea
          value={value}
          style={style}
          placeholder={placeholder}
          rows={rows}
          cols={cols}
          data-testid={testId || 'textarea-cmp'}
          onChange={onChangeTextLocal}
          disabled={disabled}
          data-testId='textarea-cmp'
        ></StyledTextarea>
      </InputContainer>
      {withError && (
        <ErrorContainer>
          {Boolean(error) && (
            <Text
              type='microMedium'
              style={{
                marginTop: theme.spacingVertical(0.2),
                fontStyle: 'italic',
                color: theme.warningDark,
              }}
            >
              {error}
            </Text>
          )}
        </ErrorContainer>
      )}
    </>
  )
}

export default Textarea
