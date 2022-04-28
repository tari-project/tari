import { TextInputProps } from './types'

import {
  StyledInput,
  IconUnitsContainer,
  InputContainer,
  UnitsText,
  IconWrapper,
} from './styles'
import { ChangeEvent } from 'react'

/**
 * @name TextInput component
 * @typedef TextInputProps
 *
 * @prop {'static' | 'disabled'} [type] - controls some input styling and behaviour
 * @prop {string} [value] - input text value
 * @prop {boolean} [hideText] - show/hide input text
 * @prop {string} [placeholder] - placeholer text
 * @prop {ReactNode} [inputIcon] - optional icon rendered inside input field
 * @prop {string} [inputUnits] - optional units text, e.g. 'MB' on right-hand side of input field
 * @prop {() => void} [onIconClick] - icon click event
 * @prop {(value: string) => void} [onChangeText] - text change event handler
 * @prop {string} [testId] - for testing purposes
 */

const TextInput = ({
  type = 'static',
  value,
  hideText = false,
  placeholder,
  inputIcon,
  inputUnits,
  onIconClick,
  onChangeText,
  testId,
}: TextInputProps) => {
  const onChangeTextLocal = (event: ChangeEvent<HTMLInputElement>) => {
    if (onChangeText) {
      onChangeText(event.target.value)
    }
  }
  return (
    <InputContainer type={type}>
      <StyledInput
        type={type}
        placeholder={placeholder}
        disabled={type === 'disabled'}
        onChange={val => onChangeTextLocal(val)}
        value={value}
        hideText={hideText}
        spellCheck={false}
        data-testid={testId || 'input-cmp'}
      />
      <IconUnitsContainer>
        {inputIcon && (
          <IconWrapper onClick={onIconClick} data-testid='icon-test'>
            {inputIcon}
          </IconWrapper>
        )}{' '}
        {inputUnits && (
          <UnitsText type='smallMedium' data-testid='units-test'>
            {inputUnits}
          </UnitsText>
        )}
      </IconUnitsContainer>
    </InputContainer>
  )
}

export default TextInput
