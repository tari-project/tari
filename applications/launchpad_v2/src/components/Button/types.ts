import { ReactNode, CSSProperties } from 'react'

export type ButtonVariantType = 'primary' | 'text'

export interface ButtonProps {
  disabled?: boolean
  children?: ReactNode
  style?: CSSProperties
  type?: 'link' | 'button' | 'submit'
  href?: string
  variant?: ButtonVariantType
  leftIcon?: ReactNode
  rightIcon?: ReactNode
  onClick?: () => void
  loading?: boolean
}
