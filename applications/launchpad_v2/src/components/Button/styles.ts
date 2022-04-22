import styled from 'styled-components'

import { ButtonProps } from './types'

export const StyledButton = styled.button<Pick<ButtonProps, 'variant'>>`
  border-radius: ${({ theme }) => theme.tightBorderRadius()};
  box-shadow: none;
  border-width: 0;
  padding: ${({ theme }) => theme.spacingVertical()}
    ${({ theme }) => theme.spacingHorizontal()};
  cursor: pointer;
  background: ${({ variant, theme }) =>
    variant === 'text' ? 'transparent' : theme.tariGradient};
  color: ${({ variant, theme }) =>
    variant === 'text' ? theme.secondary : theme.primary};
  outline: none;

  &:hover {
    background: ${({ theme }) => theme.accent};
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
