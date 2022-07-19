import { animated } from 'react-spring'
import styled from 'styled-components'
import colors from '../../styles/styles/colors'

export const SwitchContainer = styled.label<{ disable?: boolean }>`
  display: flex;
  align-items: center;
  cursor: ${({ disable }) => (disable ? '' : 'pointer')};
`

export const SwitchController = styled(animated.div)<{ disable?: boolean }>`
  height: 14px;
  width: 24px;
  border: 1.5px solid
    ${({ theme, disable }) =>
      disable ? theme.disabledText : theme.switchBorder};
  border-radius: 6px;
  position: relative;
  box-sizing: border-box;
  box-shadow: 0px 0px 2px rgba(0, 0, 0, 0.08);
  cursor: ${({ disable }) => (disable ? '' : 'pointer')};
  -webkit-box-shadow: 0px 0px 2px -1px ${colors.dark.primary};
  -moz-box-shadow: 0px 0px 2px -1px ${colors.dark.primary};
  box-shadow: 0px 0px 2px -1px ${colors.dark.primary};
`

export const SwitchCircle = styled(animated.div)<{ disable?: boolean }>`
  position: absolute;
  width: 14px;
  height: 14px;
  top: 0;
  margin-top: -1.5px;
  margin-left: -0.5px;
  border-radius: 6px;
  box-sizing: border-box;
  background: ${({ theme }) => theme.accent};
  border: 1.5px solid
    ${({ theme, disable }) =>
      disable ? theme.disabledText : theme.switchBorder};
  -webkit-box-shadow: 0px 0px 2px -1px ${colors.dark.primary};
  -moz-box-shadow: 0px 0px 2px -1px ${colors.dark.primary};
  box-shadow: 0px 0px 2px -1px ${colors.dark.primary};
`

export const LabelText = styled(animated.span)`
  font-weight: 500;
  font-size: 14px;
  line-height: 160%;
  display: flex;
  align-items: center;
  margin: 0;
`
