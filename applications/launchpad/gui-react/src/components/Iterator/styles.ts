import styled from 'styled-components'

export const Wrapper = styled.div`
  display: flex;
  align-items: center;
  border: 1px solid ${({ theme }) => theme.borderColor};
  border-radius: ${({ theme }) => theme.tightBorderRadius()};
  padding ${({ theme }) => theme.spacingVertical(0.5)} ${({ theme }) =>
  theme.spacingHorizontal()}
`
