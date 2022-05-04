import { ModalContainer, Backdrop, ModalContent } from './styles'
import type { ModalProps } from './types'

const Modal = ({ open, children, onClose, size }: ModalProps) => {
  if (!open) {
    return null
  }

  return (
    <ModalContainer>
      <Backdrop onClick={onClose} data-testid='modal-backdrop' />
      <ModalContent size={size}> {children}</ModalContent>
    </ModalContainer>
  )
}

Modal.defaultProps = {
  size: 'large',
}

export default Modal
