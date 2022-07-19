import { animated } from 'react-spring'
import styled from 'styled-components'

export const DashboardLayout = styled(animated.div)`
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: auto;
  ::-webkit-scrollbar {
    width: 15px;
  }

  /* Track */
  ::-webkit-scrollbar-track {
    background: ${({ theme }) => theme.scrollBarTrack};
  }

  /* Handle */
  ::-webkit-scrollbar-thumb {
    background: ${({ theme }) => theme.scrollBarThumb};
    border-radius: 6px;
    border: 3px solid transparent;
    background-clip: padding-box;
  }

  /* Handle on hover */
  ::-webkit-scrollbar-thumb:hover {
    background: ${({ theme }) => theme.scrollBarHover};
    border-radius: 6px;
    border: 3px solid transparent;
    background-clip: padding-box;
  }
`

export const DashboardContent = styled.div`
  flex: 1;
  padding-top: 60px;
  padding-bottom: 60px;
`
