import Backdrop from '../Backdrop'

import { ModalContainer, ModalContent } from './styles'
import type { ModalProps } from './types'

const Modal = ({ open, children, onClose, size, local, style }: ModalProps) => {
  if (!open) {
    return null
  }

  return (
    <ModalContainer local={local}>
      <Backdrop onClick={onClose} data-testid='modal-backdrop' opacity={0.4} />
      <ModalContent size={size} style={style}>
        {children}
      </ModalContent>
    </ModalContainer>
  )
}

Modal.defaultProps = {
  size: 'large',
}

export default Modal
