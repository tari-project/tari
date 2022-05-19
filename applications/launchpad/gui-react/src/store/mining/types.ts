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
  pending?: boolean
  history?: {
    timestamp?: string // UTC timestamp
    amount?: string // bignumber (?)
    chain?: string // ie. xtr, xmr aka coin/currency?
    type?: string // to enum, ie. mined, earned, received, sent
  }[]
}

export interface MiningNodeState {
  sessions?: MiningSession[]
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
  dependsOn: ContainerStateFieldsWithIdAndType[]
}
