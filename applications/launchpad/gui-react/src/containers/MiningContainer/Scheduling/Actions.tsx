import { ReactNode } from 'react'
import { useTheme } from 'styled-components'

import Box from '../../../components/Box'

/**
 * @name Actions
 * @description modified Box used as a container for scheduling modal action buttons
 *
 * @prop {ReactNode} children - Children to render
 * @prop {ReactNode} [content] - Additional content to be rendered above buttons
 */
const Actions = ({
  children: actionButtons,
  content,
}: {
  children: ReactNode
  content?: ReactNode
}) => {
  const theme = useTheme()

  return (
    <Box
      border={false}
      style={{
        width: '100%',
        borderTopLeftRadius: 0,
        borderTopRightRadius: 0,
        borderTop: `1px solid ${theme.selectBorderColor}`,
        marginBottom: 0,
        marginTop: 0,
        background: theme.nodeBackground,
      }}
    >
      {content && (
        <div
          style={{
            marginBottom: theme.spacing(),
          }}
        >
          {content}
        </div>
      )}
      <div
        style={{
          display: 'flex',
          justifyContent: 'flex-end',
          columnGap: theme.spacing(),
        }}
      >
        {actionButtons}
      </div>
    </Box>
  )
}

export default Actions
