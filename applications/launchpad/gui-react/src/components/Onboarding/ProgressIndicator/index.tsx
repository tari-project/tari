import { useEffect, useState } from 'react'
import BarSegment from './BarSegment'
import { StyledContainer } from './styles'
import { ProgressIndicatorProps } from './types'

const ProgressIndicator = ({ overallFill }: ProgressIndicatorProps) => {
  const [fillOne, setFillOne] = useState<number | undefined>(0)
  const [fillTwo, setFillTwo] = useState<number | undefined>(0)
  const [fillThree, setFillThree] = useState<number | undefined>(0)
  const [fillFour, setFillFour] = useState<number | undefined>(0)

  useEffect(() => {
    if (overallFill) {
      if (overallFill <= 0.25) {
        setFillOne(overallFill * 4)
      }
      if (overallFill > 0.25 && overallFill <= 0.5) {
        setFillOne(1)
        setFillTwo((overallFill - 0.25) * 5)
      }
      if (overallFill > 0.5 && overallFill <= 0.75) {
        setFillOne(1)
        setFillTwo(1)
        setFillThree((overallFill - 0.5) * 4)
      } else if (overallFill > 0.75) {
        setFillOne(1)
        setFillTwo(1)
        setFillThree(1)
        setFillFour((overallFill - 0.75) * 3)
      }
    }
  }, [overallFill])

  return (
    <StyledContainer>
      <BarSegment fill={fillOne} />
      <BarSegment fill={fillTwo} />
      <BarSegment fill={fillThree} />
      <BarSegment fill={fillFour} />
    </StyledContainer>
  )
}

export default ProgressIndicator
