import { config, useSpring } from 'react-spring'
import { BarSegmentContainer, AnimatedSegment } from './styles'

const BarSegment = ({ fill }: { fill: number | undefined }) => {
  let progressBarWidth
  if (fill) {
    progressBarWidth = 92 * fill
  }
  const progressAnim = useSpring({
    width: progressBarWidth,
    config: config.gentle,
  })
  return (
    <BarSegmentContainer>
      <AnimatedSegment style={{ ...progressAnim }} $fill={fill} />
    </BarSegmentContainer>
  )
}

export default BarSegment
