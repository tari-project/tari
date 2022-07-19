import styled from 'styled-components'

export const StyledSendForm = styled.form`
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: auto;
  box-sizing: border-box;
`

export const SendFormContent = styled.div`
  flex: 1;
  padding: ${({ theme }) => theme.spacing(1.67)};
  padding-bottom: ${({ theme }) => theme.spacing(0.67)};
`

export const TagWrapper = styled.div`
  display: flex;
  justify-content: center;
  margin-bottom: ${({ theme }) => theme.spacingVertical(1)};
`

export const FormButtons = styled.div`
  display: flex;
  position: relative;
  justify-content: space-between;
  padding: ${({ theme }) => `${theme.spacing(0.67)} ${theme.spacing(1.67)}`};
  column-gap: ${({ theme }) => theme.spacingHorizontal(1)};
  border-top: 1px solid ${({ theme }) => theme.selectBorderColor};
`

export const TagBox = styled.div`
  background: ${({ theme }) => theme.backgroundSecondary};
  border-radius: ${({ theme }) => theme.borderRadius()};
  padding: ${({ theme }) =>
    `${theme.spacingVertical(1)} ${theme.spacingHorizontal(1)}`};
`

export const PleaseWaitContainer = styled.div`
  padding: ${({ theme }) => theme.spacing(1.67)};
  display: flex;
  flex-direction: column;
  height: 100%;
  align-items: center;
  justify-content: center;
  row-gap: ${({ theme }) => theme.spacingVertical(0.1)};
  box-sizing: border-box;
`

export const ResultModal = styled.div`
  padding: ${({ theme }) => theme.spacing(1.67)};
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  height: 100%;
`

export const ResultModalContent = styled.div`
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
`

export const ResultHeader = styled.div`
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  row-gap: ${({ theme }) => theme.spacingVertical(1)};
  margin-bottom: ${({ theme }) => theme.spacingVertical(2)};
`

export const CtaButtonContainer = styled.div``

export const ErrorContainer = styled.div`
  cursor: pointer;
  position: absolute;
  left: 0;
  width: 100%;
  color: #fff;
  background: ${({ theme }) => theme.warningDark};
  padding: ${({ theme }) => `${theme.spacing(0.67)} ${theme.spacing(1.67)}`};
  box-sizing: border-box;
  bottom: 100%;
`
