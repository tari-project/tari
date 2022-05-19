import { CSSProperties, ReactNode } from 'react'
import { TagType } from '../../../components/Tag/types'
import { Container } from '../../../store/containers/types'
import {
  MiningContainersState,
  MiningNodeState,
} from '../../../store/mining/types'
import { MiningNodeType } from '../../../types/general'

export enum MiningBoxStatus {
  Custom = 'custom',
  SetupRequired = 'setup_required',
  Paused = 'paused',
  Running = 'running',
  Error = 'error',
}

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
    [key in MiningBoxStatus]: NodeBoxStatusConfig
  }>
  currentStatus?: MiningBoxStatus
  icons?: ReactNode[]
  children?: ReactNode
  testId?: string
  nodeState: MiningNodeState
  containersState: MiningContainersState
  containersToStopOnPause: { id: string; type: Container }[]
}
