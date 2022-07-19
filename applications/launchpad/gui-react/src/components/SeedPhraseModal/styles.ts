import styled from 'styled-components'

export const ModalContent = styled.div`
  display: flex;
  flex-direction: column;
  height: 100%;
  position: relative;
  box-sizing: border-box;
`

export const Content = styled.div`
  flex: 1;
  overflow: auto;
  padding: ${({ theme }) =>
    `${theme.spacingVertical(1.8)} ${theme.spacingHorizontal(1.65)}`};
  padding-top: ${({ theme }) => theme.spacingVertical(3.5)};
  color: ${({ theme }) => theme.primary};
`

export const BottomBar = styled.div`
  padding: ${({ theme }) =>
    `${theme.spacingVertical(1.8)} ${theme.spacingHorizontal(1.65)}`};
  border-top: 1px solid ${({ theme }) => theme.balanceBoxBorder};
  box-sizing: border-box;
  display: flex;
  column-gap: ${({ theme }) => theme.spacingHorizontal(1)};
  background: ${({ theme }) => theme.nodeBackground};
  justify-content: space-between;
  border-bottom-left-radius: ${({ theme }) => theme.borderRadius(1)};
  border-bottom-right-radius: ${({ theme }) => theme.borderRadius(1)};
`

export const ErrorContainer = styled.div`
  position: absolute;
  top: 0;
  bottom: 0;
  left: 0;
  right: 0;
  box-sizing: border-box;
  width: 100%;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  background: rgba(196, 196, 196, 0.3);
  border-radius: ${({ theme }) => theme.borderRadius(1)};
`

export const ErrorText = styled.div`
  padding: ${({ theme }) =>
    `${theme.spacingVertical(1.8)} ${theme.spacingHorizontal(1.65)}`};
  background: ${({ theme }) => theme.nodeBackground};
  color: ${({ theme }) => theme.warningDark};
`

export const TextSection = styled.div`
  margin-top: ${({ theme }) => theme.spacingVertical(2)};
  margin-bottom: ${({ theme }) => theme.spacingVertical(2)};
`

export const PrintButtonWrapper = styled.div`
  display: flex;
  justify-content: center;
  align-items: center;
  margin-top: ${({ theme }) => theme.spacingVertical(4)};
  margin-bottom: ${({ theme }) => theme.spacingVertical(1)};
  color: ${({ theme }) => theme.onTextLight};
`

export const PrintView = styled.div`
  position: fixed;
  background: #fff;
  width: 100vw;
  height: 100vh;
  left: 0;
  right: 0;
  top: 0;
  bottom: 0;
  z-index: 10000;
  display: flex;
  flex-direction: column;
  padding: 40px;
  color: #000;
  align-items: flex-start;
  justify-content: flex-start;
`

export const PrintPhrase = styled.div`
  margin-top: 30px;
`

export const WordsContainer = styled.div`
  padding-left: ${({ theme }) => theme.spacingHorizontal(1)};
  padding-right: ${({ theme }) => theme.spacingHorizontal(1)};
`

export const WordDisplay = styled.div`
  margin-top: ${({ theme }) => theme.spacingVertical(1)};
  margin-bottom: ${({ theme }) => theme.spacingVertical(1)};
  padding: ${({ theme }) => theme.spacing(0.8)};
  padding-bottom: ${({ theme }) => theme.spacing(0.6)};
  background: ${({ theme }) => theme.backgroundSecondary};
  border-radius: ${({ theme }) => theme.borderRadius()};
`

export const WordInputRow = styled.div`
  display: flex;
  align-items: center;
  margin-top: ${({ theme }) => theme.spacingVertical(2)};
  margin-bottom: ${({ theme }) => theme.spacingVertical(2)};
`

export const NumWrapper = styled.div`
  min-width: 40px;
`

export const CenteredContent = styled.div`
  display: flex;
  align-items: center;
  justify-content: center;
  flex: 1;
  height: 100%;
  box-sizing: border-box;
  padding: ${({ theme }) =>
    `${theme.spacingVertical(1.8)} ${theme.spacingHorizontal(1.65)}`};
`
