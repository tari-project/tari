import { useTheme } from 'styled-components'

import Box from '../../../components/Box'
import { BoxProps } from '../../../components/Box/types'

/**
 * @name Actions
 * @description modified Box used as a container for scheduling modal action buttons
 *
 * @param {BoxProps} props - Box props
 */
const Actions = (props: BoxProps) => {
  const theme = useTheme()

  return (
    <Box
      {...props}
      border={false}
      style={{
        width: '100%',
        borderTopLeftRadius: 0,
        borderTopRightRadius: 0,
        borderTop: `1px solid ${theme.borderColor}`,
        marginBottom: 0,
        display: 'flex',
        justifyContent: 'flex-end',
        columnGap: theme.spacing(),
      }}
    />
  )
}

export default Actions
