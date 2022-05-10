import Text from '../Text'
import { CalloutIcon, StyledCallout } from './styles'

import { CalloutProps } from './types'

/**
 * Callout component that renders styled box with proper icon.
 * NOTE: It supports only the `warning` type for now.
 *
 * @param {CalloutType} [type='warning'] - the callout type/style.
 * @param {ReactNode} [icon] - override the icon.
 * @param {string | ReactNode} children - the callout content (text or ReactNode).
 */
const Callout = ({
  type = 'warning',
  icon = '⚠️',
  inverted,
  children,
}: CalloutProps) => {
  let content = children

  if (typeof children === 'string') {
    content = (
      <Text style={{ display: 'inline' }} type='microMedium'>
        {children}
      </Text>
    )
  }

  return (
    <StyledCallout $type={type} $inverted={inverted}>
      <CalloutIcon>{icon}</CalloutIcon>
      {content}
    </StyledCallout>
  )
}

export default Callout
