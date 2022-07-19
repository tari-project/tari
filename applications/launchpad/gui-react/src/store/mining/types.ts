import { ScheduleId, CoinType } from '../../types/general'
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
  startedAt?: string
  finishedAt?: string
  id?: string
  total?: Record<string, number>
  reason: MiningActionReason
  schedule?: ScheduleId
  history: { txId: string; amount: number }[]
}

export interface MiningNodeState {
  session?: MiningSession
}

export interface MoneroUrl {
  url: string
}

export interface MergedMiningNodeState extends MiningNodeState {
  address?: string
  threads: number
  urls?: MoneroUrl[]
  useAuth: boolean
}

/**
 * @typedef BlockMinedNotification
 *
 * @prop {number} amount - amount mined
 * @prop {CoinType} currency - what currency was awarded for mining
 * @prop {string} header - header message for notification
 * @prop {string} message - additional body message for notification
 */
export interface BlockMinedNotification {
  amount: number
  header: string
  message: string
  currency: CoinType
}

export interface MiningState {
  tari: MiningNodeState
  merged: MergedMiningNodeState
  notifications: BlockMinedNotification[]
}

export interface MiningContainersState extends ContainerStateFields {
  miningPending?: boolean
  dependsOn: ContainerStateFieldsWithIdAndType[]
}
