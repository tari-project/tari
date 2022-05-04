import { createSelector } from '@reduxjs/toolkit'
import { MiningNodeType } from '../../types/general'

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
