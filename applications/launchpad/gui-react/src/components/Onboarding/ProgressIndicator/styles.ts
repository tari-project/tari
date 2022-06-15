import { animated } from 'react-spring'
import styled from 'styled-components'

export const StyledContainer = styled.div`
  width: 404px;
  display: flex;
  justify-content: space-evenly;
  margin-top: ${({ theme }) => theme.spacingVertical(4)};
`

export const BarSegmentContainer = styled(animated.div)<{ fill?: number }>`
  width: 92px;
  height: 5px;
  border-radius: ${({ theme }) => theme.borderRadius(4)};
  background-color: ${({ theme }) => theme.placeholderText};
  display: inline-block;
  position: relative;
`

export const ProgressBarSegment = styled(animated.span)`
  background-image: ${({ theme }) => theme.tariGradient};
  border-radius: ${({ theme }) => theme.borderRadius(4)};
  /* display: inline-block, */
  height: 100%;
  position: absolute;
`
