/* eslint-disable indent */
import { TabProp } from './types'
import { animated } from 'react-spring'
import styled from 'styled-components'

export const TabsContainer = styled.div`
  display: flex;
  align-items: flex-start;
  flex-direction: column;
  position: relative;
  white-space: no-wrap;
`

export const TabOptions = styled.div`
  display: flex;
  align-items: center;
`

export const Tab = styled.button<{
  $inverted?: boolean
  selected?: string
  tab?: TabProp
}>`
  display: flex;
  padding: 8px 12px;
  box-shadow: none;
  border-width: 0px;
  border-bottom: 4px solid transparent;
  border-radius: ${({ theme }) => theme.tightBorderRadius(1.5)};
  border-bottom-left-radius: ${({ theme, selected, tab }) =>
    selected === tab?.id ? 0 : theme.tightBorderRadius(1.5)};
  border-bottom-right-radius: ${({ theme, selected, tab }) =>
    selected === tab?.id ? 0 : theme.tightBorderRadius(1.5)};
  background: transparent;
  box-sizing: border-box;
  margin: 0px;
  margin-right: ${({ theme }) => `${theme.tabsMarginRight}`}px;
  position: relative;
  cursor: pointer;
  align-items: center;
  transition: ease-in-out 300ms;
  color: ${({ theme }) => theme.primary};
  &:hover {
    background-color: ${({ theme, $inverted }) =>
      $inverted
        ? theme.inverted.backgroundSecondary
        : theme.backgroundSecondary};
  }
  &:last-child {
    margin-right: 0;
  }
`

export const TabSelectedBorder = styled(animated.div)<{ $inverted?: boolean }>`
  position: absolute;
  height: 4px;
  border-radius: 2px;
  background: ${({ theme, $inverted }) =>
    $inverted ? theme.inverted.accentSecondary : theme.accent};
  bottom: 0;
`

export const FontWeightCompensation = styled.div`
  visibility: hidden;

  & > p {
    margin: 0;
  }
`

export const TabContent = styled.div`
  position: absolute;
  top: 0;
  left: 0;
  display: flex;
  padding: 12px;
  width: 100%;
  align-items: center;
  justify-content: center;
  box-sizing: border-box;

  & > p {
    margin: 0;
  }
`
