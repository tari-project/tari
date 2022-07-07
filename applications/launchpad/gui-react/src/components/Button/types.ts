import { ReactNode, CSSProperties } from 'react'

export type ButtonVariantType =
  | 'primary'
  | 'secondary'
  | 'warning'
  | 'text'
  | 'button-in-text'

export type ButtonSizeType = 'medium' | 'small'

export type ButtonType = 'link' | 'button' | 'submit'

export interface ButtonProps {
  disabled?: boolean
  children?: ReactNode
  style?: CSSProperties
  type?: ButtonType
  size?: ButtonSizeType
  href?: string
  variant?: ButtonVariantType
  leftIcon?: ReactNode
  leftIconColor?: string
  rightIcon?: ReactNode
  autosizeIcons?: boolean
  onClick?: () => void
  loading?: boolean
  fullWidth?: boolean
  testId?: string
}
