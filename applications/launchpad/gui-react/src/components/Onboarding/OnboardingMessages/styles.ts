import styled from 'styled-components'

export const CtaButtonContainer = styled.div<{ $noMargin?: boolean }>`
  display: inline-flex;
  ${({ theme, $noMargin }) =>
    !$noMargin ? `margin-top: ${theme.spacingVertical(1)};` : ''}
`

export const ActionStatusContainer = styled.div`
  display: flex;
  align-items: center;
  justify-content: center;
  column-gap: ${({ theme }) => theme.spacing(0.5)};
  margin-top: ${({ theme }) => theme.spacing()};
`

export const StatusRow = styled.div`
  display: flex;
  align-items: center;
  column-gap: ${({ theme }) => theme.spacing(0.2)};

  & > p:first-child {
    display: flex;
    margin-bottom: 2px;
  }
`
