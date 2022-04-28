import { TextInputProps } from './types'

import {
  StyledInput,
  IconUnitsContainer,
  InputContainer,
  UnitsText,
  IconWrapper,
} from './styles'
import { ChangeEvent } from 'react'

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
