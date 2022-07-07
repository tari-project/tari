import styled from 'styled-components'

export const StyledRunningButton = styled.button`
  background: ${({ theme }) => theme.resetBackground};
  border-width: 0;
  box-shadow: none;
  padding-top: ${({ theme }) => theme.spacingVertical(0.6)};
  padding-bottom: ${({ theme }) => theme.spacingVertical(0.6)};
  border-radius: ${({ theme }) => theme.borderRadius(0.667)};
  display: inline-flex;
  cursor: pointer;

  &:hover {
    background: ${({ theme }) => theme.resetBackgroundHover};
  }
`

export const TimeWrapper = styled.span`
  padding: ${({ theme }) =>
    `${theme.spacingVertical(0.2)} ${theme.spacingHorizontal(
      0.83,
    )} ${theme.spacingVertical(0)} ${theme.spacingHorizontal(0.83)}`};
  border-right: 1px solid ${({ theme }) => theme.resetBackground};
  min-width: 61px;
`

export const TextWrapper = styled.span`
  padding: ${({ theme }) =>
    `${theme.spacingVertical(0.2)} ${theme.spacingHorizontal(
      0.83,
    )} ${theme.spacingVertical(0.0)} ${theme.spacingHorizontal(0.83)}`};
  min-width: 61px;
`
