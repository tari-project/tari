import { ReactNode } from 'react'
import styled, { useTheme } from 'styled-components'

import Box from '../../../components/Box'

export const IsWalletRunningRow = ({ children }: { children: ReactNode }) => {
  const theme = useTheme()
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
        background: theme.nodeBackground,
        borderColor: theme.selectBorderColor,
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
