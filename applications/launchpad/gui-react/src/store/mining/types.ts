import { ScheduleId } from '../../types/general'
import {
  ContainerStateFields,
  Container,
  ContainerStateFieldsWithIdAndType,
} from '../containers/types'

export enum TariMiningSetupRequired {
  MissingWalletAddress = 'missing_wallet_address',
}

export enum MergedMiningSetupRequired {
  MissingWalletAddress = 'missing_wallet_address',
  MissingMoneroAddress = 'missing_monero_address',
}

export interface MiningDependencyState {
  type: Container
  running: boolean
  pending: boolean
  error: boolean
}

export enum MiningActionReason {
  Schedule = 'schedule',
  Manual = 'manual',
}

export interface MiningSession {
  startedAt?: string // UTC timestamp
  finishedAt?: string
  id?: string // uuid (?)
  total?: Record<string, string> // i,e { xtr: 1000 bignumber (?) }
  reason: MiningActionReason
  schedule?: ScheduleId
}

export interface MiningNodeState {
  session?: MiningSession
}

export interface MoneroUrl {
  url: string
  useAuth?: boolean
}

/**
 * @TODO - omit password? Probably we don't want to store password in global state.
 */
export interface MergedMiningNodeState extends MiningNodeState {
  address?: string
  threads: number
  urls?: MoneroUrl[]
  authentication?: {
    username?: string
    password?: string
  }
}

export interface MiningState {
  tari: MiningNodeState
  merged: MergedMiningNodeState
}

export interface MiningContainersState extends ContainerStateFields {
  miningPending?: boolean
  dependsOn: ContainerStateFieldsWithIdAndType[]
}
