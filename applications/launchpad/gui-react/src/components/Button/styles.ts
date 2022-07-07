/* eslint-disable indent */
import styled, { DefaultTheme, css } from 'styled-components'

import { ButtonProps, ButtonVariantType } from './types'

const getButtonBackgroundColor = ({
  disabled,
  variant,
  theme,
}: Pick<ButtonProps, 'variant' | 'disabled'> & { theme: DefaultTheme }) => {
  if ((disabled || variant === 'secondary') && variant !== 'text') {
    return theme.disabledPrimaryButton
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

const ButtonCSS = css<
  { $fullWidth?: boolean } & Pick<ButtonProps, 'variant' | 'type' | 'disabled'>
>`
display: flex;
position: relative;
${({ $fullWidth }) => $fullWidth && 'width: 100%;'}
justify-content: ${({ $fullWidth }) =>
  $fullWidth ? 'center' : 'space-between'};
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
text-decoration: none;

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

export const StyledButton = styled.button<
  { $fullWidth?: boolean } & Pick<ButtonProps, 'variant' | 'type' | 'disabled'>
>`
  ${ButtonCSS}
`

export const StyledLinkLikeButton = styled.a<
  { $fullWidth?: boolean } & Pick<ButtonProps, 'variant' | 'type' | 'disabled'>
>`
  ${ButtonCSS}
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
  $leftIconColor?: string
}>`
  display: inline-flex;
  ${({ $spacing, theme }) => {
    if ($spacing) {
      return `margin-${$spacing}: ${theme.spacingHorizontal(0.25)};`
    }

    return ''
  }}

  color: ${({ $disabled, theme, $leftIconColor }) => {
    if ($disabled) {
      return theme.disabledPrimaryButtonText
    } else if ($leftIconColor) {
      return $leftIconColor
    }

    return 'inherit'
  }};

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
  disabled?: boolean
}>`
  display: inline-flex;
  color: ${({ disabled, theme }) => {
    if (disabled) {
      return theme.disabledPrimaryButtonText
    }

    return 'inherit'
  }};
`

export const LoadingIconWrapper = styled.span`
  display: inline-flex;
  margin-left: ${({ theme }) => theme.spacingHorizontal(0.2)};
`
