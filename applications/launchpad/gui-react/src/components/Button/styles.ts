/* eslint-disable indent */
import styled, { DefaultTheme } from 'styled-components'

import { ButtonProps, ButtonVariantType } from './types'

const getButtonBackgroundColor = ({
  disabled,
  variant,
  theme,
}: Pick<ButtonProps, 'variant' | 'disabled'> & { theme: DefaultTheme }) => {
  if ((disabled || variant === 'secondary') && variant !== 'text') {
    return theme.backgroundImage
  }

  switch (variant) {
    case 'text':
      return 'transparent'
    case 'warning':
      return theme.warningGradient
    default:
      return theme.tariGradient
  }
}

export const StyledButton = styled.button<
  Pick<ButtonProps, 'variant' | 'type'>
>`
  display: flex;
  position: relative;
  justify-content: space-between;
  align-items: center;
  column-gap: 0.25em;
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

    if (variant === 'secondary') {
      return `1px solid ${theme.borderColor}`
    }

    if (variant === 'warning') {
      return `1px solid ${theme.warning}`
    }

    return `1px solid ${theme.accent}`
  }};
  box-shadow: none;
  padding: ${({ theme }) => theme.spacingVertical(0.5)}
    ${({ theme }) => theme.spacingHorizontal()};
  cursor: ${({ disabled }) => (disabled ? 'default' : 'pointer')};
  background: ${getButtonBackgroundColor};
  color: ${({ disabled, variant, theme }) => {
    if (disabled) {
      return theme.disabledText
    }

    if (variant === 'secondary') {
      return theme.primary
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

      if (variant === 'secondary') {
        return theme.backgroundSecondary
      }

      if (variant === 'warning') {
        return theme.warningDark
      }

      return theme.accent
    }};

    ${({ variant, disabled }) =>
      variant === 'text' && !disabled ? 'opacity: 0.7;' : ''}
`

export const StyledLink = styled.a<Pick<ButtonProps, 'variant' | 'disabled'>>`
  display: inline-flex;
  align-items: center;
  background: ${({ variant, theme }) =>
    variant === 'text' ? 'transparent' : theme.tariGradient};
  color: ${({ variant, theme }) =>
    variant === 'text' ? theme.secondary : theme.primary};
  cursor: pointer;
  margin: 0;
  padding: 0;
  text-decoration: underline;
  box-sizing: border-box;
  border-width: 0;
  box-shadow: none;
  font-size: inherit;
  color: inherit;
  line-height: inherit;
  font-family: inherit;
  font-weight: inherit;

  ${({ disabled }) => {
    if (disabled) {
      return `
        opacity: 0.5;
      `
    }

    return ''
  }}

  &:hover {
    opacity: ${({ disabled }) => (disabled ? '0.5' : '0.7')};
  }
`

export const StyledButtonText = styled.span<Pick<ButtonProps, 'size'>>`
  display: flex;
  padding-top: ${({ theme, size }) =>
    theme.spacingVertical(size === 'small' ? 0.1 : 0.2)};
`

export const IconWrapper = styled.span<{
  $spacing?: 'left' | 'right'
  $autosizeIcon?: boolean
  $variant?: ButtonVariantType
  $disabled?: boolean
}>`
  display: inline-flex;
  ${({ $spacing, $variant, theme }) => {
    if ($spacing) {
      const factor = $variant && $variant === 'button-in-text' ? 0.25 : 0.4
      return `margin-${$spacing}: ${theme.spacingHorizontal(factor)};`
    }

    return ''
  }}

  color: ${({ $variant, $disabled, theme }) =>
    $variant === 'text' && !$disabled ? theme.primary : 'inherit'};

  ${({ $autosizeIcon }) => {
    if ($autosizeIcon) {
      return `
        & > svg {
          width: 16px;
          height: 16px;
        }
      `
    }
    return ''
  }}
`

export const ButtonContentWrapper = styled.span<{
  $variant?: ButtonVariantType
  disabled?: boolean
}>`
  display: inline-flex;
  color: ${({ $variant, disabled, theme }) =>
    $variant === 'text' && !disabled ? theme.primary : 'inherit'};
`

export const LoadingIconWrapper = styled.span`
  display: inline-flex;
  margin-left: ${({ theme }) => theme.spacingHorizontal(0.2)};
`
