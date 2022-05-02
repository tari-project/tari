import styled from 'styled-components'

export const StyledAnchor = styled.a`
  color: ${({ theme }) => theme.accent};
  text-decoration: none;
  &:hover {
    color: ${({ theme }) => theme.accentDark};
  }
`
