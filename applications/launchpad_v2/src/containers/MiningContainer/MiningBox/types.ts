import { ReactNode } from 'react'
import { TagType } from '../../../components/Tag/types'
import { MiningNodesStatus } from '../../../store/mining/types'
import { MiningNodeType } from '../../../types/general'

export interface NodeBoxStatusConfig {
  tag: {
    text: string
    type: TagType
  }
}

export interface MiningBoxProps {
  node: MiningNodeType
  statuses?: Record<keyof MiningNodesStatus, NodeBoxStatusConfig>
  children?: ReactNode
}
