import styled from 'styled-components'

export const StyledCoinsList = styled.ul<{ color?: string; inline?: boolean }>`
  color: ${({ color }) => (color ? color : 'inherit')};
  list-style: none;
  padding-left: 0;
  margin-top: 0;
  margin-bottom: 0;
  display: ${({ inline }) => (inline ? 'inline-block' : '')};
`

export const CoinsListItem = styled.li<{ $loading?: boolean }>`
  opacity: ${({ $loading }) => ($loading ? 0.64 : 1)};
  display: flex;
  align-items: baseline;
`

export const IconWrapper = styled.span`
  margin-right: 8px;
  margin-top: -4px;
  & > svg {
    width: 24px;
    height: 24px;
  }
  color: ${({ theme }) => theme.inverted.secondary};
`
