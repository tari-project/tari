import styled from 'styled-components'

export const StyledBox = styled.span`
  background: ${({ theme }) => theme.backgroundImage};
  border: 1px solid ${({ theme }) => theme.borderColor};
  border-radius: ${({ theme }) => theme.tightBorderRadius()};
  color: ${({ theme }) => theme.secondary};
  padding: ${({ theme }) => theme.spacingVertical()}
    ${({ theme }) => theme.spacingHorizontal()};
  margin: ${({ theme }) => theme.spacingVertical()} 0;
  box-sizing: border-box;
  display: flex;
  justify-content: space-between;
`
