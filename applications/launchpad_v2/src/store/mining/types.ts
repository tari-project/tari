/**
 * @TODO - the list of possible statuses may change.
 * If so, then MiningBox and Mining Container may need to be changed as well.
 * UNKNOWN - the status of the container/node is unknown, ie. on app launch it can be default status
 * SETUP_REQUIRED - node/container cannot be run because of missing configuration (merge with BLOCKED?)
 * BLOCKED - node/container cannot be run because some requirement is not satisfied, ie. mining node needs base node running
 * PAUSED - node/container is not running. NOTE: node and container are not necessary the same. Ie. Docker container can be live, but process running inside the container can be stopped. So maybe we should split this also into PAUSED and STOPPED?
 * RUNNING - node/container is running and healthy
 * ERROR - node/container is failed
 */
export enum MiningNodesStatus {
  'UNKNOWN' = 'UNKNOWN',
  'SETUP_REQUIRED' = 'SETUP_REQUIRED',
  'BLOCKED' = 'BLOCKED',
  'PAUSED' = 'PAUSED',
  'RUNNING' = 'RUNNING',
  'ERROR' = 'ERROR',
}

export enum CodesOfTariMining {
  'MISSING_WALLET_ADDRESS',
}

export enum CodesOfMergedMining {
  'MISSING_WALLET_ADDRESS',
  'MISSING_MONERO_ADDRESS',
}

export interface MiningSession {
  startedAt?: string // UTC timestamp
  finishedAt?: string
  id?: string // uuid (?)
  total?: Record<string, string> // i,e { xtr: 1000 bignumber (?) }
  history?: {
    timestamp?: string // UTC timestamp
    amount?: string // bignumber (?)
    chain?: string // ie. xtr, xmr aka coin/currency?
    type?: string // to enum, ie. mined, earned, received, sent
  }[]
}

export interface MiningNodeState<TDisabledReason> {
  pending: boolean
  status: MiningNodesStatus
  disabledReason?: TDisabledReason
  sessions?: MiningSession[]
}

export type TariMiningNodeState = MiningNodeState<CodesOfTariMining>
export type MergedMiningNodeState = MiningNodeState<CodesOfMergedMining>

export type MiningNodeStates = TariMiningNodeState | MergedMiningNodeState

export interface MiningState {
  tari: TariMiningNodeState
  merged: MergedMiningNodeState
}
