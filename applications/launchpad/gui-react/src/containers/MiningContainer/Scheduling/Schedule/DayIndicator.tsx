import { ReactNode } from 'react'

import { StyledIndicatorContainer, EnabledDot } from './styles'

/**
 * @name DayIndicator
 * @description renders day of the week with additional coloring and dot when enabled
 *
 * @prop {boolean} enabled - indicates whether the day is enabled or not
 * @prop {ReactNode} children - day description/capital
 * @prop {boolean} disabled - indicates whether to render in disabled UI state
 */
const DayIndicator = ({
  enabled,
  children,
  disabled,
}: {
  enabled: boolean
  children: ReactNode
  disabled: boolean
}) => {
  return (
    <StyledIndicatorContainer enabled={enabled} disabled={disabled}>
      <>
        {enabled && <EnabledDot disabled={disabled} />}
        {children}
      </>
    </StyledIndicatorContainer>
  )
}

export default DayIndicator
