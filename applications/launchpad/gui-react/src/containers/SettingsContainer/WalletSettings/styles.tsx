import { ReactNode } from 'react'
import styled from 'styled-components'

import Box from '../../../components/Box'

export const IsWalletRunningRow = ({ children }: { children: ReactNode }) => {
  return (
    <Box
      style={{
        borderRadius: 0,
        borderLeft: 'none',
        borderRight: 'none',
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'center',
        paddingLeft: 0,
        paddingRight: 0,
      }}
    >
      {children}
    </Box>
  )
}

export const WalletRunningContainer = styled.span`
  display: flex;
  align-items: baseline;
  column-gap: ${({ theme }) => theme.spacingVertical()};
`
