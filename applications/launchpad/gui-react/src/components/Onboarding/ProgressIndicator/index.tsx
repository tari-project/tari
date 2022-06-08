import { BarSegment, StyledContainer } from './styles'
import { ProgressIndicatorProps } from './types'

const ProgressIndicator = ({ fill }: ProgressIndicatorProps) => {
  return (
    <StyledContainer>
      <BarSegment fill={fill} />
      <BarSegment />
      <BarSegment />
      <BarSegment />
    </StyledContainer>
  )
}

export default ProgressIndicator
