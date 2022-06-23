import styled from 'styled-components'

export const DockerRow = styled.div`
  display: flex;
  padding: ${({ theme }) => theme.spacingVertical()};
  &:not(:last-of-type) {
    border-bottom: 1px solid ${({ theme }) => theme.borderColor};
  }
`
