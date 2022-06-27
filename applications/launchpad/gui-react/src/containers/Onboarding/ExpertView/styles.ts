import styled from 'styled-components'

export const Wrapper = styled.div`
  display: flex;
  flex-direction: column;
  padding-left: ${({ theme }) => theme.spacing()};
  height: 100%;
`

export const ScrollContainer = styled.div`
  margin-top: ${({ theme }) => theme.spacing()};
  flex-grow: 1;
  overflow: auto;
  padding-right: ${({ theme }) => theme.spacing()};
`
