import styled from 'styled-components'

export const DockerRow = styled.div<{ $inverted?: boolean }>`
  display: flex;
  align-items: center;
  height: 2em;
  padding: ${({ theme }) => theme.spacingVertical(1.25)};
  &:not(:last-of-type) {
    border-bottom: 1px solid
      ${({ theme, $inverted }) =>
        $inverted ? theme.inverted.resetBackground : theme.selectBorderColor};
  }
`

export const DockerList = styled.div`
  position: relative;
`

export const DockerStatusWrapper = styled.div`
  flex-grow: 1;
  display: flex;
  width: 70%;
  align-items: center;
  justify-content: flex-end;
  column-gap: ${({ theme }) => theme.spacingHorizontal(0.5)};
`

export const ErrorWrapper = styled.span`
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 60%;
  overflow: hidden;
  cursor: pointer;
  color: ${({ theme }) => theme.error};

  &:hover {
    text-decoration: underline;
  }
`

export const ProgressContainer = styled.div`
  display: flex;
  flex-direction: column;
  justify-content: flex-start;
  width: 80%;
`

export const TextProgessContainer = styled.span`
  font-size: 12px;
  text-overflow: ellipsis;
  max-width: 100%;
  overflow: hidden;
`
