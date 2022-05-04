import { animated, useSpring } from 'react-spring'
import { useTheme } from 'styled-components'

import Text from '../Text'

import {
  LabelText,
  SwitchContainer,
  SwitchController,
  SwitchCircle,
} from './styles'
import { SwitchProps } from './types'

/**
 * Switch input controller
 *
 * @param {boolean} value - the input value
 * @param {string | ReactNode} [leftLabel] - the text or ReactNode element on the left side of the switch.
 * @param {string | ReactNode} [rightLabel] - the text or ReactNode element on the right side of the switch.
 * @param {(val: boolean) => void} onClick - when the switch is clicked. Returns the new value.
 * @param {boolean} [inverted] - use inverted styles
 * @param {string} [testId] - the test ID (react-testing/jest)
 *
 * @example
 * <Switch
 *  leftLabel={<SvgSun width='1.4em' height='1.4em' />}
 *  rightLabel={'The label text'}
 *  value={currentTheme === 'dark'}
 *  onClick={v => dispatch(setTheme(v ? 'dark' : 'light'))}
 * />
 */
const Switch = ({
  value,
  leftLabel,
  rightLabel,
  onClick,
  inverted,
  testId,
}: SwitchProps) => {
  const theme = useTheme()

  const themeStyle = inverted ? theme.inverted : theme

  const circleAnim = useSpring({
    left: value ? 10 : -1,
  })

  const labelColorAnim = useSpring({
    color: themeStyle.primary,
  })

  const controllerAnim = useSpring({
    background: value ? themeStyle.accent : 'transparent',
  })

  const leftLabelEl =
    leftLabel && typeof leftLabel === 'string' ? (
      <Text as={animated.span} type='smallMedium' style={{ ...labelColorAnim }}>
        {leftLabel}
      </Text>
    ) : (
      leftLabel
    )
  const rightLabelEl =
    rightLabel && typeof rightLabel === 'string' ? (
      <Text as={animated.span} type='smallMedium' style={{ ...labelColorAnim }}>
        {rightLabel}
      </Text>
    ) : (
      rightLabel
    )

  return (
    <SwitchContainer
      onClick={() => onClick && onClick(!value)}
      data-testid={testId || 'switch-input-cmp'}
    >
      {leftLabelEl ? (
        <LabelText
          style={{
            marginRight: 12,
            ...labelColorAnim,
          }}
        >
          {leftLabelEl}
        </LabelText>
      ) : null}

      <SwitchController style={{ ...controllerAnim }}>
        <SwitchCircle style={{ ...circleAnim }} />
      </SwitchController>

      {rightLabelEl ? (
        <LabelText
          style={{
            marginLeft: 12,
            ...labelColorAnim,
          }}
        >
          {rightLabelEl}
        </LabelText>
      ) : null}
    </SwitchContainer>
  )
}

export default Switch
