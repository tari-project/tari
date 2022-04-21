import { useContext } from 'react'
import { useSpring } from 'react-spring'
import { ThemeContext } from 'styled-components'

import {
  LabelText,
  SwitchContainer,
  SwitchController,
  SwitchCircle,
} from './styles'
import { SwitchProps } from './types'

/**
 * Switch input controller
 * @param {SwitchProps} props
 */
const Switch = ({
  value,
  label,
  onClick,
  invertedStyle,
  testId,
}: SwitchProps) => {
  const theme = useContext(ThemeContext)

  const themeStyle = invertedStyle ? theme.inverted : theme

  const circleAnim = useSpring({
    left: value ? 10 : -1,
  })

  const labelColorAnim = useSpring({
    color: themeStyle.primary,
  })

  const controllerAnim = useSpring({
    background: value ? themeStyle.accent : 'transparent',
  })

  return (
    <SwitchContainer
      onClick={() => onClick && onClick(!value)}
      data-testid={testId || 'switch-input-cmp'}
    >
      <SwitchController style={{ ...controllerAnim }}>
        <SwitchCircle style={{ ...circleAnim }} />
      </SwitchController>

      {label ? (
        <LabelText
          style={{
            ...labelColorAnim,
          }}
        >
          {label}
        </LabelText>
      ) : null}
    </SwitchContainer>
  )
}

export default Switch
