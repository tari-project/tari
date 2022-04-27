/* eslint-disable indent */
import styled, { DefaultTheme } from 'styled-components'

import { ButtonProps } from './types'

const getButtonBackgroundColor = ({
  disabled,
  variant,
  theme,
}: Pick<ButtonProps, 'variant' | 'disabled'> & { theme: DefaultTheme }) => {
  if (disabled) {
    return theme.backgroundImage
  }

  return variant === 'text' ? 'transparent' : theme.tariGradient
}

export const StyledButton = styled.button<
  Pick<ButtonProps, 'variant' | 'type'>
>`
  display: flex;
  position: relative;
  justify-content: space-between;
  align-items: baseline;
  margin: 0;
  border-radius: ${({ theme }) => theme.tightBorderRadius()};
  border: ${({ disabled, theme, variant }) => {
    if (variant === 'text') {
      return 'none'
    }

    if (disabled) {
      return `1px solid ${getButtonBackgroundColor({
        disabled,
        theme,
        variant,
      })}`
    }

    return `1px solid ${theme.accent}`
  }};
  box-shadow: none;
  padding: ${({ theme }) => theme.spacingVertical()}
    ${({ theme }) => theme.spacingHorizontal()};
  cursor: ${({ disabled }) => (disabled ? 'default' : 'pointer')};
  background: ${getButtonBackgroundColor};
  color: ${({ disabled, variant, theme }) => {
    if (disabled) {
      return theme.disabledText
    }

    return variant === 'text' ? theme.secondary : theme.inverted.primary
  }};
  outline: none;

  & * {
    color: inherit;
  }

  &:hover {
    background: ${({ disabled, variant, theme }) => {
      if (disabled || variant === 'text') {
        return 'auto'
      }

      return theme.accent
    }};
  }
`

export const StyledLink = styled.a<Pick<ButtonProps, 'variant'>>`
  background: ${({ variant, theme }) =>
    variant === 'text' ? 'transparent' : theme.tariGradient};
  color: ${({ variant, theme }) =>
    variant === 'text' ? theme.secondary : theme.primary};
`

export const ButtonText = styled.span``

export const IconWrapper = styled.span``

export const LoadingIconWrapper = styled.span`
  margin-left: 0.25em;
`
