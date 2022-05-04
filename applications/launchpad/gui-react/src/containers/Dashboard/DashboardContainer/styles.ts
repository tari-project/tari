import { animated } from 'react-spring'
import styled from 'styled-components'

export const DashboardLayout = styled(animated.div)`
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: auto;
`

export const DashboardContent = styled.div`
  flex: 1;
  padding-top: 60px;
  padding-bottom: 60px;
`
