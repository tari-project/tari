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
  box-shadow: ${({ theme }) => theme.shadow2};
  padding: 40px;
  color: ${({ theme }) => theme.primary};
`

export const DotsContainer = styled.div`
  display: flex;
  flex-direction: row;
  justify-content: flex-end;
  padding-right: ${({ theme }) => theme.spacingHorizontal(0.6)};
  margin-top: -30px;
  margin-bottom: -15px;
`
