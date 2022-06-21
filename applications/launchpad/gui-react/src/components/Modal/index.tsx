import Backdrop from '../Backdrop'

import { ModalContainer, ModalContent } from './styles'
import type { ModalProps } from './types'

const Modal = ({ open, children, onClose, size, local }: ModalProps) => {
  if (!open) {
    return null
  }

  return (
    <ModalContainer local={local}>
      <Backdrop onClick={onClose} data-testid='modal-backdrop' />
      <ModalContent size={size}>{children}</ModalContent>
    </ModalContainer>
  )
}

Modal.defaultProps = {
  size: 'large',
}

export default Modal
