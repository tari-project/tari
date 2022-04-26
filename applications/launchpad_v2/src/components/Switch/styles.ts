import { animated } from 'react-spring'
import styled from 'styled-components'
import colors from '../../styles/styles/colors'

export const SwitchContainer = styled.label`
  display: flex;
  align-items: center;
  cursor: pointer;
`

export const SwitchController = styled(animated.div)`
  height: 14px;
  width: 24px;
  border: 1.5px solid ${colors.dark.primary};
  border-radius: 6px;
  position: relative;
  box-sizing: border-box;
  box-shadow: 0px 0px 2px rgba(0, 0, 0, 0.08);
  cursor: pointer;
`

export const SwitchCircle = styled(animated.div)`
  position: absolute;
  width: 14px;
  height: 14px;
  top: 0;
  margin-top: -1px;
  border-radius: 6px;
  box-sizing: border-box;
  background: #fff;
  border: 1.5px solid ${colors.dark.primary};
`

export const LabelText = styled(animated.span)`
  font-weight: 500;
  font-size: 14px;
  line-height: 160%;
  display: flex;
  align-items: center;
  margin: 0;
`
