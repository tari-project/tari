import { useEffect, useRef, useState } from 'react'
import { config, useSpring } from 'react-spring'
import Text from '../Text'
import { Track, Fill, StyledProgressBar, Tip } from './styles'

import { ProgressBarProps } from './types'

const limitValue = (value: number) => {
  return value > 100 ? 100 : Math.abs(value)
}

/**
 * Linear progress bar with a tip.
 * @param {number} value - number from 0-100 range
 */
const ProgressBar = ({ value }: ProgressBarProps) => {
  const trackRef = useRef<HTMLDivElement | null>(null)

  const [width, setWidth] = useState(value)

  useEffect(() => {
    if (trackRef.current) {
      const trackWidth = trackRef.current.clientWidth
      setWidth(Math.round((trackWidth * limitValue(value)) / 100))
    }
  }, [value])

  const progressAnim = useSpring({
    width: width,
    config: config.gentle,
  })

  const tipAnim = useSpring({
    left: width,
    config: config.gentle,
  })

  return (
    <StyledProgressBar>
      <Track ref={trackRef}>
        <Fill style={{ ...progressAnim }} $filled={value === 100} />
        <Tip style={{ ...tipAnim }} className='progressbar-tip'>
          <Text as='span' type='smallMedium'>
            {limitValue(value)}%
          </Text>
        </Tip>
      </Track>
    </StyledProgressBar>
  )
}

export default ProgressBar
