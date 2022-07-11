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

export const FlexContent = styled.div`
  display: flex;
  flex-direction: column;
`

export const ProgressContainer = styled.div`
  display: flex;
  flex-direction: column;
  align-items: center;
  margin-top: ${({ theme }) => theme.spacingVertical(4)};
  margin-left: auto;
  margin-right: auto;
  width: 100%;
  max-width: 450px;
`

export const RemainingTime = styled.div`
  margin-top: ${({ theme }) => theme.spacingVertical(1.5)};
  margin-bottom: ${({ theme }) => theme.spacingVertical(2)};
`

export const CalcRemainTimeCont = styled.div`
  display: flex;
`

export const CalcRemainTimeContLoader = styled.div`
  padding-top: ${({ theme }) => theme.spacingVertical(0.18)};
  margin-right: ${({ theme }) => theme.spacingHorizontal(0.4)};
`
