import { ReactNode } from 'react'
import { useTheme } from 'styled-components'

import Modal from '../Modal'
import Box from '../Box'
import Button from '../Button'
import Text from '../Text'
import t from '../../locales'

/**
 * @name Alert
 * @description A simple modal component showing a message with optional title
 *
 * @prop {ReactNode} content - content shown in the alert
 * @prop {boolean} open - indicates whether alert should be shown or not
 * @prop {() => void} onClose - callback on close action of the alert (Close button and backdrop)
 * @prop {string} [title] - optional title of the alert
 *
 * @example
 * <Alert
 *   content='Something went wrong'
 *   open={isOpen}
 *   onClose={() => setIsOpen(false)}
 * />
 */
const Alert = ({
  content,
  open,
  onClose,
  title,
}: {
  content: ReactNode
  open: boolean
  onClose: () => void
  title?: string
}) => {
  const theme = useTheme()

  return (
    <Modal open={open} onClose={onClose} size='auto'>
      <Box
        border={false}
        style={{ wordBreak: 'break-all', background: 'transparent' }}
      >
        {Boolean(title) && (
          <Text type='subheader' style={{ marginTop: `-${theme.spacing()}` }}>
            {title}
          </Text>
        )}
        {content}
      </Box>
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
          background: 'transparent',
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
