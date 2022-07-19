import { createSelector } from '@reduxjs/toolkit'
import { RootState } from '..'
import { selectContainerStatusWithStats } from '../containers/selectors'
import { Container } from '../containers/types'
import { selectWalletSetupRequired } from '../wallet/selectors'
import {
  MergedMiningSetupRequired,
  MiningContainersState,
  TariMiningSetupRequired,
} from './types'

export const selectNotifications = (r: RootState) =>
  r.mining.notifications || []

/**
 * ============== TARI ===================
 */
export const selectTariMiningState = (r: RootState) => r.mining.tari

export const selectTariContainers = createSelector(
  selectContainerStatusWithStats(Container.Tor),
  selectContainerStatusWithStats(Container.BaseNode),
  selectContainerStatusWithStats(Container.Wallet),
  selectContainerStatusWithStats(Container.SHA3Miner),
  (tor, baseNode, wallet, sha3) => {
    const containers = [tor, baseNode, wallet, sha3]
    const errors = containers
      .filter(c => c.error)
      .map(c => ({
        containerName: c.containerName,
        id: c.id,
        error: c.error,
      }))

    return {
      running: containers.every(c => c.running),
      pending: containers.some(c => c.pending),
      miningPending: sha3.pending,
      error: errors.length > 0 ? errors : undefined,
      dependsOn: [tor, baseNode, wallet, sha3],
    } as MiningContainersState
  },
  {
    memoizeOptions: {
      equalityCheck: (a, b) => {
        return (
          a.running === b.running &&
          a.pending === b.pending &&
          a.error === b.error &&
          a.id === b.id
        )
      },
    },
  },
)

export const selectTariSetupRequired = createSelector(
  selectWalletSetupRequired,
  walletSetupRequired =>
    walletSetupRequired
      ? TariMiningSetupRequired.MissingWalletAddress
      : undefined,
)

/**
 * ============== MERGED ===================
 */
export const selectMergedMiningState = (r: RootState) => r.mining.merged
export const selectMergedMiningAddress = (r: RootState) =>
  r.mining.merged.address
export const selectMergedMiningThreads = (r: RootState) =>
  r.mining.merged.threads
export const selectMoneroUrls = (r: RootState) =>
  (r.mining.merged.urls || []).map(u => u.url).join(',')
export const selectMergedUseAuth = (r: RootState) => r.mining.merged.useAuth

export const selectMergedContainers = createSelector(
  selectContainerStatusWithStats(Container.Tor),
  selectContainerStatusWithStats(Container.BaseNode),
  selectContainerStatusWithStats(Container.Wallet),
  selectContainerStatusWithStats(Container.MMProxy),
  selectContainerStatusWithStats(Container.XMrig),
  (tor, baseNode, wallet, mmproxy, xmrig) => {
    const containers = [tor, baseNode, wallet, mmproxy, xmrig]
    const errors = containers
      .filter(c => c.error)
      .map(c => ({
        containerName: c.containerName,
        id: c.id,
        error: c.error,
      }))

    return {
      running: !containers.some(c => !c.running),
      pending: containers.some(c => c.pending),
      miningPending: mmproxy.pending || xmrig.pending,
      error: errors.length > 0 ? errors : undefined,
      dependsOn: [tor, baseNode, wallet, xmrig, mmproxy],
    } as MiningContainersState
  },
  {
    memoizeOptions: {
      equalityCheck: (a, b) => {
        return (
          a.running === b.running &&
          a.pending === b.pending &&
          a.error === b.error &&
          a.id === b.id
        )
      },
    },
  },
)

export const selectMergedSetupRequired = createSelector(
  selectWalletSetupRequired,
  selectMergedMiningAddress,
  (walletSetupRequired, mergedAddress) => {
    if (walletSetupRequired) {
      return MergedMiningSetupRequired.MissingWalletAddress
    }

    if (!mergedAddress || mergedAddress.length < 1) {
      return MergedMiningSetupRequired.MissingMoneroAddress
    }

    return
  },
)

/**
 * ============== OTHER ===================
 */

/**
 * Can any mining node be run?
 */
export const selectCanAnyMiningNodeRun = (state: RootState) => {
  const tari = selectTariContainers(state)
  const merged = selectMergedContainers(state)
  const tariSetupRequired = selectTariSetupRequired(state)
  const mergedSetupRequired = selectMergedSetupRequired(state)

  return (
    (!tari.error && !tariSetupRequired) ||
    (!merged.error && !mergedSetupRequired)
  )
}

/**
 * Is any mining node running?
 * @returns {boolean}
 */
export const selectIsMiningRunning = (state: RootState): boolean => {
  const tari = selectTariContainers(state)
  const merged = selectMergedContainers(state)

  return tari.running || merged.running
}

/**
 * Is any mining node pending?
 */
export const selectIsMiningPending = (state: RootState): boolean => {
  const tari = selectTariContainers(state)
  const merged = selectMergedContainers(state)

  return !!(tari?.miningPending || merged?.miningPending)
}
