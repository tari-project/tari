import styled from 'styled-components'

export const StyledCoinsList = styled.ul<{ color?: string }>`
  color: ${({ color }) => (color ? color : 'inherit')};
  list-style: none;
  padding-left: 0;
  margin-top: 0;
`

export const CoinsListItem = styled.li<{ loading?: boolean }>`
  opacity: ${({ loading }) => (loading ? 0.64 : 1)};
  display: flex;
  align-items: center;
`
