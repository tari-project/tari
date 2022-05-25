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

export interface MiningSession {
  startedAt?: string // UTC timestamp
  finishedAt?: string
  id?: string // uuid (?)
  total?: Record<string, string> // i,e { xtr: 1000 bignumber (?) }
}

export interface MiningNodeState {
  session?: MiningSession
}

export interface MergedMiningNodeState extends MiningNodeState {
  address?: string
  threads?: number
  urls?: string[]
}

export interface MiningState {
  tari: MiningNodeState
  merged: MergedMiningNodeState
}

export interface MiningContainersState extends ContainerStateFields {
  miningPending?: boolean
  dependsOn: ContainerStateFieldsWithIdAndType[]
}
