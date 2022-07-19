import styled from 'styled-components'

export const DockerDwnlTagContainer = styled.div`
  display: flex;
  margin-bottom: ${({ theme }) => theme.spacingVertical(2)};
`

export const DockerDwnlTag = styled.div<{ $dark: boolean }>`
  background: ${({ theme, $dark }) =>
    $dark ? theme.inverted.backgroundSecondary : theme.backgroundSecondary};
  color: ${({ theme, $dark }) => ($dark ? '#fff' : theme.accentDark)};
  border-radius: ${({ theme }) => theme.borderRadius(1)};
  width: 100%;
  display: flex;
  flex: 1;
  text-align: center;
  align-items: center;
  padding-left: ${({ theme }) => theme.spacingHorizontal(0.5)};
`

export const DockerDwnlInnerTag = styled.span`
  background: ${({ theme }) => theme.warning};
  color: ${({ theme }) => theme.warningText};
  border-radius: ${({ theme }) => theme.borderRadius(1)};
  padding: ${({ theme }) =>
    `${theme.spacingVertical(0.23)} ${theme.spacingHorizontal(0.5)}`};
`

export const ButtonsContainer = styled.div`
  display: flex;
  width: 100%;
  align-items: center;
  column-gap: ${({ theme }) => theme.spacingHorizontal(1)};
  margin-top: ${({ theme }) => theme.spacingVertical(2)};
  height: 38px;
`

export const ProgressContainer = styled.div`
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  margin-top: ${({ theme }) => theme.spacingVertical(2)};
  height: 38px;
  color: ${({ theme }) => theme.onTextLight};
`
