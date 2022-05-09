import { createSelector } from '@reduxjs/toolkit'
import { RootState } from '..'
import { MiningNodeType } from '../../types/general'
import { MiningNodesStatus } from './types'

/**
 * Get Redux state of the given mining node
 * @example
 * const miningState = useAppSelector(state => selectMiningNode(state, 'merged'))
 */
export const selectMiningNode = createSelector(
  [state => state.mining, (_, node: MiningNodeType) => node],
  (miningState, node) => miningState[node],
)

/**
 * Get stats for last/current session
 */
export const selectLastSession = createSelector(
  [state => state.mining, (_, node: MiningNodeType) => node],
  (miningState, node) =>
    miningState[node].sessions && miningState[node].sessions.length > 0
      ? miningState[node].sessions[miningState[node].sessions.length - 1]
      : undefined,
)

/**
 * Select the Tari mining status
 * @returns {boolean}
 */
export const selectTariMiningStatus = (state: RootState) =>
  state.mining.tari.status

/**
 * Is any mining able to run?
 * (Is not in unknown, error or setup_required state)
 */
export const selectCanAnyMiningNodeRun = (state: RootState) =>
  [MiningNodesStatus.RUNNING, MiningNodesStatus.PAUSED].includes(
    state.mining.tari.status,
  ) ||
  [MiningNodesStatus.RUNNING, MiningNodesStatus.PAUSED].includes(
    state.mining.merged.status,
  )
