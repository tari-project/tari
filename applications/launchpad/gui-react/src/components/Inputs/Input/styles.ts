/* eslint-disable indent */
import { InputHTMLAttributes } from 'react'
import styled from 'styled-components'

import Text from '../../Text'

export const StyledInput = styled.input<InputHTMLAttributes<HTMLInputElement>>`
  height: 100%;
  width: 100%;
  padding: 0px 16px;
  font-family: 'AvenirMedium';
  font-size: 14px;
  line-height: inherit;
  color: ${({ theme, disabled }) => {
    if (disabled) {
      return theme.placeholderText
    } else {
      return theme.primary
    }
  }};
  background-color: ${({ theme, disabled }) =>
    disabled ? theme.backgroundImage : theme.background};
  border: none;
  border-radius: 8px;
  ::placeholder {
    color: ${({ theme }) => theme.inputPlaceholder};
  }
  &:focus {
    outline: none;
    color: ${({ theme }) => {
      return theme.primary
    }};
  }
`

export const InputContainer = styled.div<{
  disabled?: boolean
  $error: boolean
  $withError?: boolean
}>`
  height: 42px;
  width: 369px;
  line-height: 42px;
  display: flex;
  align-items: center;
  background-color: ${({ theme, disabled }) =>
    disabled ? theme.backgroundImage : theme.background};
  border: 1px solid;
  border-color: ${({ theme }) => theme.borderColor};
  border-radius: 8px;
  font-family: 'AvenirMedium';
  margin-bottom: ${({ $withError, $error, theme }) =>
    $error || !$withError ? '0' : theme.spacingVertical(1.6)};
  :focus-within {
    outline: none;
    border-color: ${({ theme }) => theme.accent};
  }
`

export const IconUnitsContainer = styled.div<{ $iconWrapperWidth: number }>`
  width: ${({ $iconWrapperWidth }) => $iconWrapperWidth}px;
  height: auto;
  display: flex;
  justify-content: center;
  align-items: center;
  margin-right: 10px;
`

export const IconWrapper = styled.div<{ onClick?: () => void }>`
  display: flex;
  cursor: ${({ onClick }) => (onClick ? 'pointer' : 'default')};
  font-size: 20px;
  color: ${({ theme }) => theme.secondary};
`

export const UnitsText = styled(Text)`
  color: ${({ theme }) => theme.placeholderText};
  text-transform: uppercase;
`

export const Label = styled.label<{ $inverted?: boolean; $noMargin?: boolean }>`
  font-size: 0.88em;
  display: inline-block;
  margin-bottom: ${({ theme, $noMargin }) =>
    $noMargin ? '0px' : theme.spacingVertical()};
  color: ${({ theme, $inverted }) =>
    $inverted ? theme.inverted.primary : theme.primary};
  font-family: 'AvenirMedium';
`
