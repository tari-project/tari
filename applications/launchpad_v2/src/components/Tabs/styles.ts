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

export const Tab = styled.button`
  display: flex;
  padding: 8px 12px;
  box-shadow: none;
  border-width: 0px;
  border-bottom: 4px solid transparent;
  background: transparent;
  box-sizing: border-box;
  margin: 0px;
  position: relative;
  cursor: pointer;
  align-items: center;
`

export const TabSelectedBorder = styled(animated.div)`
  position: absolute;
  height: 4px;
  border-radius: 2px;
  background: ${({ theme }) => theme.accent};
  bottom: 0;
`

export const FontWeightCompensation = styled.div`
  visibility: hidden;

  & > p {
    margin: 0;
  }
`

export const TabContent = styled.div<{ selected?: boolean }>`
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
