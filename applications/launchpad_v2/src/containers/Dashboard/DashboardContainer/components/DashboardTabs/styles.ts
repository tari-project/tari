import styled from 'styled-components'

export const StyledTabContent = styled.div`
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-bottom: 4px;
`

export const TabMainText = styled.div<{ spacingRight?: boolean }>`
  margin-top: 4px;
  margin-right: ${({ spacingRight }) => (spacingRight ? '8px' : 0)};
`

export const TabTagSubText = styled.span`
  opacity: 0.5;
`

export const LoadingWrapper = styled.div`
  display: flex;
  margin-left: 8px;
  margin-top: 6px;
  opacity: 0.5;
`
