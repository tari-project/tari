import { ReactNode, CSSProperties } from 'react'

export type ButtonVariantType = 'primary' | 'text' | 'disabled'

export interface ButtonProps {
  disabled?: boolean
  children?: ReactNode
  style?: CSSProperties
  type?: 'link' | 'button' | 'submit' | 'reset'
  href?: string
  variant?: ButtonVariantType
  leftIcon?: ReactNode
  rightIcon?: ReactNode
  onClick?: () => void
  loading?: boolean
}
