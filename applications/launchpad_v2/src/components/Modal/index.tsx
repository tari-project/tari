import { ReactNode } from 'react'
import { useTheme } from 'styled-components'

const Modal = ({
  open,
  children,
  onClose,
  size,
}: {
  open?: boolean
  children: ReactNode
  onClose: () => void
  size?: 'large' | 'small'
}) => {
  const theme = useTheme()

  if (!open) {
    return null
  }

  return (
    <div
      style={{
        position: 'fixed',
        top: 0,
        bottom: 0,
        left: 0,
        right: 0,
        zIndex: 9001,
        display: 'flex',
        justifyContent: 'center',
        alignItems: 'center',
      }}
    >
      <div
        style={{
          background: theme.secondary,
          opacity: 0.1,
          position: 'absolute',
          top: 0,
          bottom: 0,
          right: 0,
          left: 0,
          zIndex: 1,
        }}
        onClick={onClose}
      />
      <div
        style={{
          width: size === 'large' ? 880 : 449,
          height: 642,
          maxWidth: '80vw',
          maxHeight: '80vh',
          background: theme.background,
          borderRadius: theme.borderRadius(),
          opacity: 1,
          zIndex: 2,
          padding: theme.spacing(),
          boxSizing: 'border-box',
          boxShadow: theme.shadow,
        }}
      >
        {children}
      </div>
    </div>
  )
}

Modal.defaultProps = {
  size: 'large',
}

export default Modal
