import { ReactNode } from 'react'
import { CSSProperties } from 'styled-components'

export type ButtonVariantType = 'primary' | 'text'

export interface ButtonProps {
  children?: ReactNode
  style?: CSSProperties
  type?: 'link' | 'button' | 'submit' | 'reset'
  href?: string
  variant?: ButtonVariantType
  leftIcon?: ReactNode
  rightIcon?: ReactNode
  onClick?: () => void
}