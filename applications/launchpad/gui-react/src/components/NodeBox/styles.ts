import styled from 'styled-components'

export const BoxHeader = styled.div`
  height: 36px;
`

export const BoxContent = styled.div`
  padding-top: ${({ theme }) => theme.spacingVertical(1)};
  padding-bottom: ${({ theme }) => theme.spacingVertical(1)};
  min-height: 136px;
  display: flex;
  flex-direction: column;
`

export const NodeBoxPlacholder = styled.div`
  display: flex;
  flex: 1;
  padding-top: ${({ theme }) => theme.spacingVertical(1)};
  padding-bottom: ${({ theme }) => theme.spacingVertical(1)};
`
