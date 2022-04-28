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
      />
      <IconUnitsContainer>
        {inputIcon && (
          <IconWrapper onClick={onIconClick}>{inputIcon}</IconWrapper>
        )}{' '}
        {inputUnits && <UnitsText type='smallMedium'>{inputUnits}</UnitsText>}
      </IconUnitsContainer>
    </InputContainer>
  )
}

export default TextInput
