import styled from 'styled-components'

// Preserve the space in the modal for the bottom bar with buttons.
const SettingsBottomBarHeight = '70px'

export const MainContainer = styled.div`
  display: flex;
  flex-direction: column;
  height: 100%;
`

export const MainContentContainer = styled.div`
  display: flex;
  flex-grow: 1;
  max-height: calc(100% - ${SettingsBottomBarHeight});
`

export const Sidebar = styled.aside`
  width: 160px;
  min-width: 160px;
  height: 100%;
  border-right: 1px solid ${({ theme }) => theme.balanceBoxBorder};
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
`

export const SidebarTabs = styled.div`
  width: 160px;
  min-width: 160px;
  border-right: 1px solid ${({ theme }) => theme.balanceBoxBorder};
  padding-top: ${({ theme }) => theme.spacing(2)};
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  row-gap: ${({ theme }) => theme.spacing(0.75)};
  justify-content: space-between;
  align-items: flex-end;
`

export const SidebarSelectTheme = styled.div`
  padding: ${({ theme }) => theme.spacingVertical(0.67)} 0;
  padding-left: ${({ theme }) => theme.spacingHorizontal()};
  box-sizing: border-box;
  width: 136px;
  align-self: flex-end;
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
    active ? theme.settingsMenuItemActive : 'none'};
  box-sizing: border-box;
  padding: ${({ theme }) => theme.spacingVertical(0.67)} 0;
  padding-left: ${({ theme }) => theme.spacingHorizontal()};
  width: 136px;
  color: ${({ theme, active }) =>
    active ? theme.accent : theme.settingsMenuItem};
  height: 39px;

  &:hover {
    background: ${({ theme }) => theme.settingsMenuItemActive};
  }
`

export const MainContent = styled.main`
  position: relative;
  flex-grow: 1;
  max-height: 100%;
  padding-top: ${({ theme }) => theme.spacing(2)};
  display: flex;
  flex-direction: column;
  justify-content: flex-start;
  align-items: center;
  overflow: auto;
  & > * {
    max-width: 100%;
    width: 70%;
  }
`

export const Footer = styled.footer`
  display: flex;
  justify-content: flex-end;
  align-items: center;
  padding: ${({ theme }) => theme.spacingVertical()};
  ${({ theme }) => theme.spacingHorizontal()};
  column-gap: ${({ theme }) => theme.spacing()};
  border-top: 1px solid ${({ theme }) => theme.selectBorderColor};
`

export const DiscardWarning = styled.div`
  display: flex;
  flex: 1;
  justify-content: center;
  gap: ${({ theme }) => theme.spacingHorizontal(0.2)};
`

export const SettingsHeader = styled.div`
  margin-bottom: ${({ theme }) => theme.spacingVertical(2.5)};
`

export const RowSpacedBetween = styled.div`
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin: ${({ theme }) => theme.spacingVertical(0.6)} 0;
`

export const SwitchBg = styled.div<{ $transparent?: boolean }>`
  display: inline-flex;
  background: ${({ theme, $transparent }) =>
    $transparent ? 'transparent' : theme.backgroundImage};
  margin-top: ${({ theme }) => theme.spacingVertical(0.67)};
  padding: ${({ theme }) => theme.spacingVertical(0.5)};
  border-radius: ${({ theme }) => theme.borderRadius(0.67)};
`

export const RowFlex = styled.div`
  display: flex;
  align-tiems: center;
  column-gap: ${({ theme }) => theme.spacingHorizontal(0.5)};
`

export const StyledList = styled.ul`
  padding-left: 20px;
`
