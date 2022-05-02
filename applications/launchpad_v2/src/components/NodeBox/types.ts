import { ReactNode } from 'react'
import { CSSWithSpring } from '../../types/general'
import { TagType } from '../Tag/types'

export interface NodeBoxProps {
  title?: string
  tag?: {
    text: string
    type?: TagType
  }
  style?: CSSWithSpring
  titleStyle?: CSSWithSpring
  contentStyle?: CSSWithSpring
  children?: ReactNode
}

export interface NodeBoxContentPlaceholderProps {
  children: string | ReactNode
}
