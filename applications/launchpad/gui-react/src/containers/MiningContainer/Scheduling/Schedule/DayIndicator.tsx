import { ReactNode } from 'react'

import { StyledIndicatorContainer, EnabledDot } from './styles'

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
