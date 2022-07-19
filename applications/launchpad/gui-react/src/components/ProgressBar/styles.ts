import { animated } from 'react-spring'
import styled from 'styled-components'

export const StyledProgressBar = styled.div`
  width: 100%;
  &:hover .progressbar-tip {
    opacity: 1;
  }
`

export const Track = styled.div`
  width: 100%;
  height: 8px;
  border-radius: ${({ theme }) => theme.borderRadius(4)};
  background-color: ${({ theme }) => theme.placeholderText};
  display: inline-block;
  position: relative;
`

export const Fill = styled(animated.div)<{ $filled?: boolean }>`
  background-image: ${({ theme }) => theme.tariGradient};
  border-top-left-radius: ${({ theme }) => theme.borderRadius(4)};
  border-bottom-left-radius: ${({ theme }) => theme.borderRadius(4)};
  border-top-right-radius: ${({ theme, $filled }) =>
    $filled ? theme.borderRadius(1) : theme.borderRadius(4)};
  border-bottom-right-radius: ${({ theme, $filled }) =>
    $filled ? theme.borderRadius(1) : theme.borderRadius(4)};
  height: 100%;
  position: absolute;
`

export const Tip = styled(animated.div)`
  opacity: 0;
  display: flex;
  justify-content: center;
  align-items: center;
  position: absolute;
  background: ${({ theme }) => theme.accent};
  color: #fff;
  width: 64px;
  height: 34px;
  top: -50px;
  margin-left: -32px;
  border-radius: ${({ theme }) => theme.borderRadius(0.5)};
  box-shadow: 0px 6.00823px 6.00823px rgba(50, 50, 71, 0.08),
    0px 6.00823px 12.0165px rgba(50, 50, 71, 0.06);

  &:after {
    content: '';
    border-right: 12px solid transparent;
    border-left: 12px solid transparent;
    position: absolute;
    border-top: 12px solid ${({ theme }) => theme.accent};
    width: 0;
    height: 0;
    left: 50%;
    margin-left: -12px;
    bottom: -8px;
    border-radius: 2px;
  }
`
