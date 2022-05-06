import { ReactNode, CSSProperties } from 'react'

export type ButtonVariantType = 'primary' | 'secondary' | 'text'

export interface ButtonProps {
  disabled?: boolean
  children?: ReactNode
  style?: CSSProperties
  type?: 'link' | 'button-in-text' | 'button' | 'submit'
  href?: string
  variant?: ButtonVariantType
  leftIcon?: ReactNode
  rightIcon?: ReactNode
  onClick?: () => void
  loading?: boolean
  testId?: string
}
