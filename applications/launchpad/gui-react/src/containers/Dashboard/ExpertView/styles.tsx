import { ReactNode, CSSProperties } from 'react'
import styled from 'styled-components'

import Box from '../../../components/Box'

export const TabsContainer = styled.div`
  display: flex;
  justify-content: space-between;
  align-items: center;
`

export const ExpertBox = ({
  children,
  style,
}: {
  children: ReactNode
  style?: CSSProperties
}) => (
  <Box
    border={false}
    style={{
      background: 'none',
      width: '100%',
      borderRadius: 0,
      ...style,
    }}
  >
    {children}
  </Box>
)
