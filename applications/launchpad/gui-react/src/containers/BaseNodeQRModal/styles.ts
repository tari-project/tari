import styled from 'styled-components'

export const ModalContainer = styled.div`
  padding: ${({ theme }) => theme.spacing(1.7)};
  display: flex;
  flex-direction: column;
  height: 100%;
  box-sizing: border-box;
  overflow: auto;
`

export const Content = styled.div`
  flex: 1;
`

export const CtaButton = styled.div`
  margin-top: ${({ theme }) => theme.spacingVertical(3)};
`

export const Instructions = styled.div`
  margin-top: ${({ theme }) => theme.spacingVertical(2.2)};
  margin-bottom: ${({ theme }) => theme.spacingVertical(3)};
  color: ${({ theme }) => theme.primary};
`

export const Steps = styled.ol`
  margin-top: ${({ theme }) => theme.spacingVertical(1.6)};
  padding-left: ${({ theme }) => theme.spacingVertical(2)};
  font-size: 14px;
`

export const QRContainer = styled.div`
  display: flex;
  justify-content: center;
  align-items: center;
  margin: 20px;
`
