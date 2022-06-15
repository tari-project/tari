import { animated, config, useSpring } from 'react-spring'
import { BarSegmentContainer, ProgressBarSegment } from './styles'

const BarSegment = ({ fill }: { fill: number | undefined }) => {
  let progressBarWidth
  if (fill) {
    progressBarWidth = 92 * fill
  }
  const progressAnim = useSpring({
    width: progressBarWidth,
    config: config.default,
  })
  // console.log('FILL: ', fill)
  return (
    <BarSegmentContainer>
      <ProgressBarSegment
        style={{
          ...progressAnim,
          // backgroundColor: 'red',
          // display: 'inline-block',
          // height: '100%',
          // position: 'absolute',
        }}
      />
    </BarSegmentContainer>
  )
}

export default BarSegment
