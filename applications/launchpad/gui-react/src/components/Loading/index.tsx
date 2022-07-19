import { CSSProperties } from 'react'

import LoadingIcon from '../../styles/Icons/Loading'

import { StyledSpan } from './styles'

/**
 * Loading
 * renders a spinning loading indicator
 *
 * @prop {boolean} loading - controls whether the indicator should be shown or not
 * @prop {string} [testId] - optional testId to assign for testing purposes
 */

const Loading = ({
  loading,
  size = '20px',
  color,
  testId,
  style,
}: {
  loading?: boolean
  size?: string
  testId?: string
  color?: string
  style?: CSSProperties
}) =>
  loading ? (
    <StyledSpan data-testid={testId || 'loading-indicator'} style={style}>
      <LoadingIcon width={size} height={size} color={color} />
    </StyledSpan>
  ) : null

export default Loading
