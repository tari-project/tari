import styled from 'styled-components'

export const DockerRow = styled.div`
  display: flex;
  padding: ${({ theme }) => theme.spacingVertical()};
  &:not(:last-of-type) {
    border-bottom: 1px solid ${({ theme }) => theme.borderColor};
  }
`

export const DockerList = styled.div`
  min-height: 3em;
`

export const DockerStatusWrapper = styled.div`
  display: flex;
  column-gap: ${({ theme }) => theme.spacingHorizontal(0.5)};
`
