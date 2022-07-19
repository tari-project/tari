import { animated } from 'react-spring'
import styled from 'styled-components'

export const TITLE_BAR_HEIGHT = 60

export const TitleBar = styled(animated.header)`
  height: ${TITLE_BAR_HEIGHT}px;
  user-select: none;
  display: flex;
  justify-content: space-between;
  align-items: center;
  position: fixed;
  z-index: 10;
  top: 0;
  left: 0;
  right: 0;
  border-top-left-radius: 10px;
  border-top-right-radius: 10px;
`

export const LeftCol = styled.div`
  flex: 1;
  display: flex;
  position: absolute;
  top: 0;
  left: 0;
  z-index: 2;
  width: 100%;
  height: 100%;
  justifycontent: space-between;
  align-items: center;
  padding-left: 16px;
  padding-right: 16px;
`

export const RightCol = styled.div``

export const WindowButtons = styled.div`
  display: flex;
  align-items: center;
`

export const TitleBarButton = styled.button<{
  borderColor: string
  background: string
}>`
  margin: 0px;
  padding: 3px;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  box-shadow: none;
  border-width: 1px;
  border-style: solid;
  background: ${({ background }) => background};
  border-color: ${({ borderColor }) => borderColor};
  display: flex;
  align-items: center;
  justify-content: center;
  margin-right: 4px;
  margin-left: 4px;
  cursor: pointer;

  ${WindowButtons}:hover & {
    svg {
      opacity: 1 !important;
    }
  }
`

export const LogoContainer = styled(animated.div)`
  padding: 4px 32px;
`
