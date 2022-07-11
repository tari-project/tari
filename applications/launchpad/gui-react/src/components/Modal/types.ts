import { CSSProperties, ReactNode } from 'react'

export interface ModalProps {
  open?: boolean
  children: ReactNode
  onClose?: () => void
  size?: 'large' | 'small' | 'auto'
  local?: boolean
  style?: CSSProperties
}
