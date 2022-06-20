import styled from 'styled-components'

export const ContentWrapper = styled.div`
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  align-items: center;
  height: 100%;
  padding: ${({ theme }) => theme.spacing(2)};
  padding-top: 0;
  box-sizing: border-box;
`

export const MessageWrapper = styled.div`
  display: flex;
  flex-grow: 1;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  row-gap: ${({ theme }) => theme.spacing(0.5)};
`
