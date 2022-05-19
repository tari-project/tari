import { createSelector } from '@reduxjs/toolkit'
import { RootState } from '..'
import { selectContainerWithMemo } from '../containers/selectors'
import { Container } from '../containers/types'
import { selectWalletSetupRequired } from '../wallet/selectors'
import {
  MergedMiningSetupRequired,
  MiningContainersState,
  TariMiningSetupRequired,
} from './types'

/**
 * ============== TARI ===================
 */
export const selectTariMiningState = (r: RootState) => r.mining.tari

export const selectTariContainers = createSelector(
  selectContainerWithMemo(Container.Tor),
  selectContainerWithMemo(Container.BaseNode),
  selectContainerWithMemo(Container.Wallet),
  selectContainerWithMemo(Container.SHA3Miner),
  (tor, baseNode, wallet, sha3) => {
    const containers = [tor, baseNode, wallet, sha3]
    const errors = containers
      .filter(c => c.error)
      .map(c => ({
        type: c.type,
        id: c.id,
        error: c.error,
      }))

    return {
      running: !containers.some(c => !c.running),
      pending: containers.some(c => c.pending),
      error: errors.length > 0 ? errors : undefined,
      dependsOn: [tor, baseNode, wallet, sha3],
    } as MiningContainersState
  },
  {
    memoizeOptions: {
      equalityCheck: (a, b) => JSON.stringify(a) === JSON.stringify(b),
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

export const selectMergedContainers = createSelector(
  selectContainerWithMemo(Container.Tor),
  selectContainerWithMemo(Container.BaseNode),
  selectContainerWithMemo(Container.Wallet),
  selectContainerWithMemo(Container.MMProxy),
  selectContainerWithMemo(Container.XMrig),
  (tor, baseNode, wallet, mmproxy, xmrig) => {
    const containers = [tor, baseNode, wallet, mmproxy, xmrig]
    const errors = containers
      .filter(c => c.error)
      .map(c => ({
        type: c.type,
        id: c.id,
        error: c.error,
      }))

    return {
      running: !containers.some(c => !c.running),
      pending: containers.some(c => c.pending),
      error: errors.length > 0 ? errors : undefined,
      dependsOn: [tor, baseNode, wallet, xmrig, mmproxy],
    } as MiningContainersState
  },
  {
    memoizeOptions: {
      equalityCheck: (a, b) => JSON.stringify(a) === JSON.stringify(b),
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
