import styled from 'styled-components'

export const TabsContainer = styled.div`
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding-right: ${({ theme }) => theme.spacing()};
  padding-left: ${({ theme }) => theme.spacing()};
  padding-bottom: ${({ theme }) => theme.spacing()};
`

export const PageContentContainer = styled.div`
  overflow: auto;
  padding ${({ theme }) => theme.spacing()};
  padding-top: 0;
`
