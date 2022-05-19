import styled from 'styled-components'
import { animated } from 'react-spring'

export const StyledMessage = styled(animated.div)`
  max-width: 385px;
  height: fit-content;
  margin-left: ${({ theme }) => theme.spacingHorizontal(0.6)};
  margin-right: ${({ theme }) => theme.spacingHorizontal(0.6)};
  margin-bottom: ${({ theme }) => theme.spacingHorizontal(0.6)};
  background-color: ${({ theme }) => theme.background};
  opacity: 1;
  border-radius: ${({ theme }) => theme.borderRadius(2)};
  box-shadow: ${({ theme }) => theme.shadow24};
  padding: 40px;
  color: ${({ theme }) => theme.primary};
  &:last-child {
    margin-bottom: 0;
  }
`
