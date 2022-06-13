import { animated } from 'react-spring'
import styled from 'styled-components'

export const ScreenContainer = styled.div`
  overflow: hidden;
  border-radius: 10px;
  flex: 1;
`

export const MainLayoutContainer = styled.div`
  position: relative;
  display: flex;
  height: 100%;
  flex: 1;
  top: 0;
  bottom: 0;
  border-radius: 10px;
  overflow: hidden;
`

export const MainContainer = styled(animated.div)`
  width: 100%;
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  padding-top: ${({ theme }) => theme.spacing(2.5)};
`

export const ExpertViewBackgroundOverlay = styled(animated.div)`
  position: absolute;
  top: 0;
  bottom: 0;
  right: 0;
  overflow: hidden;
  background: ${({ theme }) => theme.inverted.background};
`

export const ExpertViewDrawer = styled(animated.div)`
  position: absolute;
  top: 0;
  bottom: 0;
  right: 0;
  padding-left: 10px;
  padding-right: 10px;
  padding-top: ${({ theme }) => theme.spacing(2.5)};
  box-sizing: border-box;
`
