import { CSSProperties, ReactNode } from 'react'
import { TagType } from '../../../components/Tag/types'
import {
  MiningContainersState,
  MiningNodeState,
} from '../../../store/mining/types'
import { MiningNodeType } from '../../../types/general'
import { WithRequiredCredentials } from '../../PasswordPrompt/useWithPasswordPrompt'

export enum MiningBoxStatus {
  Custom = 'custom',
  SetupRequired = 'setup_required',
  Paused = 'paused',
  PausedNoSession = 'paused_no_session',
  Running = 'running',
  Error = 'error',
}

export interface NodeBoxStatusConfig {
  title?: string
  tag?: {
    content: string | ReactNode
    type?: TagType
  }
  boxStyle?: CSSProperties
  titleStyle?: CSSProperties
  contentStyle?: CSSProperties
  icon?: {
    color: string
  }
  helpSvgGradient?: boolean
}

export interface MiningCoinIconProp {
  coin: string
  component: ReactNode
}

export interface MiningBoxProps {
  node: MiningNodeType
  statuses?: Partial<{
    [key in MiningBoxStatus]: NodeBoxStatusConfig
  }>
  currentStatus?: MiningBoxStatus
  icons?: MiningCoinIconProp[]
  children?: ReactNode
  testId?: string
  nodeState: MiningNodeState
  containersState: MiningContainersState
  helpMessages?: string[]
  requiredAuthentication?: WithRequiredCredentials
}
