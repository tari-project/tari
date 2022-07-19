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
  flex-grow: 1;
  padding: ${({ theme }) => theme.spacing()};
  padding-top: 0;
`

export const ScrollablePageContentContainer = styled.div`
  flex-grow: 1;
  overflow: auto;
  padding: ${({ theme }) => theme.spacing()};
  padding-top: 0;
`
