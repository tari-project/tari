import { CSSProperties, ReactNode } from 'react'
import { TagType } from '../../../components/Tag/types'
import { MiningNodesStatus } from '../../../store/mining/types'
import { MiningNodeType } from '../../../types/general'

export interface NodeBoxStatusConfig {
  title?: string
  tag?: {
    text: string
    type?: TagType
  }
  boxStyle?: CSSProperties
  titleStyle?: CSSProperties
  contentStyle?: CSSProperties
  icon?: {
    color: string
  }
}

export interface MiningBoxProps {
  node: MiningNodeType
  statuses?: Partial<{
    [key in keyof typeof MiningNodesStatus]: NodeBoxStatusConfig
  }>
  icons?: ReactNode[]
  children?: ReactNode
  testId?: string
}
