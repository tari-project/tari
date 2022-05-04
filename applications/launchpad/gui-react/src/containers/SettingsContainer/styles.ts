import styled from 'styled-components'

export const MainContainer = styled.div`
  display: flex;
  flex-direction: column;
  height: 100%;
`

export const MainContentContainer = styled.div`
  display: flex;
  flex-grow: 1;
`

export const Sidebar = styled.aside`
  width: 160px;
  min-width: 160px;
  height: 100%;
  border-right: 1px solid ${({ theme }) => theme.borderColor};
  padding-top: ${({ theme }) => theme.spacing(2)};
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  row-gap: ${({ theme }) => theme.spacing(0.75)};
  align-items: flex-end;
`

export const MenuItem = styled.button<{ active?: boolean }>`
  margin: 0;
  padding: 0;
  outline: none;
  border: none;
  text-align: left;
  cursor: pointer;
  border-top-left-radius: ${({ theme }) => theme.tightBorderRadius(0.75)};
  border-bottom-left-radius: ${({ theme }) => theme.tightBorderRadius(0.75)};
  background: ${({ theme, active }) =>
    active ? theme.backgroundImage : 'none'};
  box-sizing: border-box;
  padding: ${({ theme }) => theme.spacingVertical()} 0;
  padding-left: ${({ theme }) => theme.spacingHorizontal()};
  width: 136px;
  color: ${({ theme, active }) => (active ? theme.accent : theme.accentDark)};

  &:hover {
    background: ${({ theme }) => theme.backgroundImage};
    color: ${({ theme }) => theme.accent};
  }
`

export const MainContent = styled.main`
  flex-grow: 1;
  padding-top: ${({ theme }) => theme.spacing(2)};
  display: flex;
  flex-direction: column;
  justify-content: flex-start;
  align-items: center;
  & > * {
    max-width: 100%;
    width: 70%;
  }
`

export const Footer = styled.footer`
  display: flex;
  justify-content: flex-end;
  align-items: center;
  padding: ${({ theme }) => theme.spacingVertical()}
    ${({ theme }) => theme.spacingHorizontal()};
  column-gap: ${({ theme }) => theme.spacing()};
  border-top: 1px solid ${({ theme }) => theme.borderColor};
`
