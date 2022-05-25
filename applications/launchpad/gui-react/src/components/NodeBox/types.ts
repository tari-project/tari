import { ReactNode } from 'react'
import { CSSWithSpring } from '../../types/general'
import { TagType } from '../Tag/types'

export interface NodeBoxProps {
  title?: string
  tag?: {
    content: string | ReactNode
    type?: TagType
  }
  style?: CSSWithSpring
  titleStyle?: CSSWithSpring
  contentStyle?: CSSWithSpring
  children?: ReactNode
  onHelpPromptClick?: () => void
  testId?: string
}

export interface NodeBoxContentPlaceholderProps {
  children: string | ReactNode
  testId?: string
}
