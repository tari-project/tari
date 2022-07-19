import styled from 'styled-components'

export const StyledTransactionsList = styled.div``

export const Header = styled.div`
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: ${({ theme }) =>
    `${theme.spacingVertical(1.23)} ${theme.spacingHorizontal(0.333)}`};
  margin-top: ${({ theme }) => theme.spacingVertical(5)};
`

export const LeftHeader = styled.div`
  display: flex;
  column-gap: ${({ theme }) => theme.spacing(0.2)};
`

export const RightHeader = styled.div``

export const PaginationContainer = styled.div`
  margin-top: ${({ theme }) => theme.spacingVertical(5)};
`
