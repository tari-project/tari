import { useTheme } from 'styled-components'

import Box from '../../../components/Box'

const Actions = (props: any) => {
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
