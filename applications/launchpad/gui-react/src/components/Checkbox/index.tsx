import { CSSProperties } from 'react'
import { useTheme } from 'styled-components'

import TickIcon from '../../styles/Icons/Tick'
import Text from '../Text'

import { Wrapper, CheckWrapper } from './styles'

/**
 * @name Checkbox
 * @description renders a controlled checkbox component with a label
 *
 * @prop {boolean} checked - whether to show checked or not checked ui state
 * @prop {(v: boolean) => void} onChange - when state changes, this callback is called with new value
 * @prop {string} children - label shown next to the tick box
 * @prop {CSSProperties} [style] - allows to extend main wrapper element styles
 * @prop {boolean} disabled - indicates whether to render in disabled UI state
 *
 * @example
 * <Checkbox
 *   checked={enabled}
 *   onChange={setEnabled}
 * >
 *   enabled
 * </Checkbox>
 */
const Checkbox = ({
  checked,
  onChange,
  children,
  style,
  disabled,
}: {
  checked: boolean
  onChange: (v: boolean) => void
  children: string
  style?: CSSProperties
  disabled?: boolean
}) => {
  const theme = useTheme()

  const color = disabled
    ? theme.placeholderText
    : checked
    ? theme.primary
    : theme.nodeWarningText

  return (
    <Wrapper
      disabled={disabled}
      style={style}
      onClick={() => onChange(!checked)}
    >
      <CheckWrapper checked={checked} disabled={disabled}>
        {checked && (
          <TickIcon color={theme.accent} width='0.9em' height='0.9em' />
        )}
      </CheckWrapper>
      <Text as='label' style={{ cursor: 'pointer' }} color={color}>
        {children}
      </Text>
    </Wrapper>
  )
}

export default Checkbox
