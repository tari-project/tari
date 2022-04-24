import styled from 'styled-components'

import { ButtonProps } from './types'

export const StyledButton = styled.button<
  Pick<ButtonProps, 'variant' | 'type'>
>`
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  border-radius: ${({ theme }) => theme.tightBorderRadius()};
  border: ${({ theme, variant, type }) => {
    if (variant === 'text') {
      return 'none'
    }

    if (type === 'reset') {
      return '1px solid transparent'
    }

    return `1px solid ${theme.accent}`
  }};
  box-shadow: none;
  padding: ${({ theme }) => theme.spacingVertical()}
    ${({ theme }) => theme.spacingHorizontal()};
  cursor: pointer;
  background: ${({ variant, type, theme }) => {
    if (type === 'reset') {
      return theme.resetBackground
    }

    return variant === 'text' ? 'transparent' : theme.tariGradient
  }};
  color: ${({ variant, theme }) =>
    variant === 'text' ? theme.secondary : theme.primary};
  outline: none;

  & * {
    color: inherit;
  }

  &:hover {
    background: ${({ variant, theme, type }) => {
      if (variant === 'text') {
        return 'auto'
      }

      if (type === 'reset') {
        return theme.resetBackgroundDark
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

export const IconWrapper = styled.span`
  color: inherit;
  width: 0;
  height: 1em;
  position: relative;
  & > * {
    position: absolute;
    top: 0;
    left: 100%;
    width: ${({ theme }) => theme.spacing(0.66)};
    height: ${({ theme }) => theme.spacing(0.66)};
    transform: translateY(-50%);
    color: inherit;
  }
`
