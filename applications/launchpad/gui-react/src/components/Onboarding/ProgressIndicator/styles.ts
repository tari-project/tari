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

export const AnimatedSegment = styled(animated.span)<{ $fill?: number }>`
  background-image: ${({ theme }) => theme.tariGradient};
  border-top-left-radius: ${({ theme }) => theme.borderRadius(4)};
  border-bottom-left-radius: ${({ theme }) => theme.borderRadius(4)};
  border-top-right-radius: ${({ theme, $fill }) =>
    $fill !== 1 ? theme.borderRadius(1) : theme.borderRadius(4)};
  border-bottom-right-radius: ${({ theme, $fill }) =>
    $fill !== 1 ? theme.borderRadius(1) : theme.borderRadius(4)};
  height: 100%;
  position: absolute;
`
