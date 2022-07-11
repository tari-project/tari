import { ReactNode } from 'react'
import { CSSProperties } from 'styled-components'

export type TagVariantType = 'small' | 'large'
export type TagType = 'info' | 'running' | 'warning' | 'expert' | 'light'

export interface TagProps {
  children?: ReactNode
  style?: CSSProperties
  type?: TagType
  variant?: TagVariantType
  icon?: ReactNode
  subText?: ReactNode
  inverted?: boolean
  dark?: boolean
  darkAlt?: boolean
  expertSec?: boolean
}
