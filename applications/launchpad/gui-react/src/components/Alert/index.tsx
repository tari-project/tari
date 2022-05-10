import { ReactNode } from 'react'
import { useTheme } from 'styled-components'

import Modal from '../Modal'
import Box from '../Box'
import Button from '../Button'
import t from '../../locales'

const Alert = ({
  content,
  open,
  onClose,
}: {
  content: ReactNode
  open: boolean
  onClose: () => void
}) => {
  const theme = useTheme()

  return (
    <Modal open={open} onClose={onClose} size='auto'>
      <Box border={false}>{content}</Box>
      <Box
        border={false}
        style={{
          marginBottom: 0,
          marginTop: 0,
          borderTopLeftRadius: 0,
          borderTopRightRadius: 0,
          borderTop: `1px solid ${theme.borderColor}`,
          display: 'flex',
          justifyContent: 'flex-end',
        }}
      >
        <Button variant='secondary' onClick={onClose}>
          {t.common.verbs.close}
        </Button>
      </Box>
    </Modal>
  )
}

export default Alert
