import styled from 'styled-components'

export const DockerRow = styled.div`
  display: flex;
  align-items: center;
  height: 2em;
  padding: ${({ theme }) => theme.spacingVertical(1.25)};
  &:not(:last-of-type) {
    border-bottom: 1px solid ${({ theme }) => theme.borderColor};
  }
`

export const DockerList = styled.div`
  position: relative;
  margin-bottom: ${({ theme }) => theme.spacing()};
`

export const DockerStatusWrapper = styled.div`
  flex-grow: 1;
  display: flex;
  width: 70%;
  align-items: center;
  column-gap: ${({ theme }) => theme.spacingHorizontal(0.5)};
`
