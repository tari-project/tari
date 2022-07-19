import { animated } from 'react-spring'
import styled from 'styled-components'

export const StyledFooter = styled.footer`
  height: 60px;
  min-height: 60px;
  display: flex;
  justify-content: center;
  align-items: center;
`

export const FooterTextWrapper = styled(animated.div)`
  text-align: center;
  color: ${({ theme }) => theme.tertiary};
`
