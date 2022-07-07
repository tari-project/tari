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
 * @param {boolean} [disable] - disable switch interaction
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
  disable,
  testId,
}: SwitchProps) => {
  const theme = useTheme()

  const themeStyle = inverted ? theme.inverted : theme

  const circleAnim = useSpring({
    left: value ? 10 : -1,
    background: value ? themeStyle.primary : themeStyle.switchController,
  })

  const leftLabelColorAnim = useSpring({
    color: disable ? themeStyle.disabledText : themeStyle.primary,
    opacity: value && leftLabel && rightLabel ? 0.5 : 1,
  })

  const rightLabelColorAnim = useSpring({
    color: disable ? themeStyle.disabledText : themeStyle.primary,
    opacity: value || !leftLabel || !rightLabel ? 1 : 0.5,
  })

  const controllerAnim = useSpring({
    background: value ? themeStyle.accent : themeStyle.switchController,
  })

  const leftLabelEl =
    leftLabel && typeof leftLabel === 'string' ? (
      <Text
        as={animated.span}
        type='smallMedium'
        style={{ ...leftLabelColorAnim }}
      >
        {leftLabel}
      </Text>
    ) : (
      leftLabel
    )
  const rightLabelEl =
    rightLabel && typeof rightLabel === 'string' ? (
      <Text
        as={animated.span}
        type='smallMedium'
        style={{ ...rightLabelColorAnim }}
      >
        {rightLabel}
      </Text>
    ) : (
      rightLabel
    )

  return (
    <SwitchContainer
      onClick={() => onClick && !disable && onClick(!value)}
      disable={disable}
      data-testid={testId || 'switch-input-cmp'}
    >
      {leftLabelEl ? (
        <LabelText
          style={{
            marginRight: 12,
            ...leftLabelColorAnim,
          }}
        >
          {leftLabelEl}
        </LabelText>
      ) : null}

      <SwitchController style={{ ...controllerAnim }} disable={disable}>
        <SwitchCircle style={{ ...circleAnim }} disable={disable} />
      </SwitchController>

      {rightLabelEl ? (
        <LabelText
          style={{
            marginLeft: 12,
            ...rightLabelColorAnim,
          }}
        >
          {rightLabelEl}
        </LabelText>
      ) : null}
    </SwitchContainer>
  )
}

export default Switch
