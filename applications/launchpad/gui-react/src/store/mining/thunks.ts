import { createAsyncThunk } from '@reduxjs/toolkit'
import { MiningNodeType } from '../../types/general'

/**
 * Start given mining node
 * @prop {NodeType} node - the node name, ie. 'tari', 'merged'
 * @returns {Promise<void>}
 */
export const startMiningNode = createAsyncThunk<void, { node: MiningNodeType }>(
  'mining/startNode',
  async ({ node }) => {
    console.log(`starting ${node} node`)
    return await new Promise(resolve => setTimeout(resolve, 2000))
  },
)

/**
 * Stop given mining node
 * @prop {NodeType} node - the node name, ie. 'tari', 'merged'
 * @returns {Promise<void>}
 */
export const stopMiningNode = createAsyncThunk<void, { node: MiningNodeType }>(
  'mining/stopNode',
  async ({ node }) => {
    console.log(`stopping ${node} node`)
    return await new Promise(resolve => setTimeout(resolve, 2000))
  },
)
