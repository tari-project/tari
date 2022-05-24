import { createAsyncThunk } from '@reduxjs/toolkit'

import { MiningNodeType } from '../../types/general'
import { selectContainerStatus } from '../containers/selectors'
import { actions as containersActions } from '../containers'
import { actions as miningActions } from './index'
import { Container } from '../containers/types'

import { RootState } from '..'

/**
 * Start given mining node. It spawns all dependencies if needed.
 * @prop {NodeType} node - the node name, ie. 'tari', 'merged'
 * @returns {Promise<void>}
 */
export const startMiningNode = createAsyncThunk<
  void,
  { node: MiningNodeType },
  { state: RootState }
>('mining/startNode', async ({ node }, thunkApi) => {
  try {
    const rootState = thunkApi.getState()

    const torStatus = selectContainerStatus(Container.Tor)(rootState)
    const baseNodeStatus = selectContainerStatus(Container.BaseNode)(rootState)
    const walletStatus = selectContainerStatus(Container.Wallet)(rootState)

    if (!torStatus.running && !torStatus.pending) {
      await thunkApi.dispatch(containersActions.start(Container.Tor)).unwrap()
    }

    if (!baseNodeStatus.running && !baseNodeStatus.pending) {
      await thunkApi
        .dispatch(containersActions.start(Container.BaseNode))
        .unwrap()
    }

    if (!walletStatus.running && !walletStatus.pending) {
      await thunkApi
        .dispatch(containersActions.start(Container.Wallet))
        .unwrap()
    }

    switch (node) {
      case 'tari':
        await thunkApi
          .dispatch(containersActions.start(Container.SHA3Miner))
          .unwrap()
        await thunkApi.dispatch(miningActions.startNewSession({ node }))
        break
      case 'merged':
        await thunkApi
          .dispatch(containersActions.start(Container.MMProxy))
          .unwrap()
        await thunkApi
          .dispatch(containersActions.start(Container.XMrig))
          .unwrap()
        await thunkApi.dispatch(miningActions.startNewSession({ node }))
        break
      default:
        break
    }
  } catch (e) {
    return thunkApi.rejectWithValue(e)
  }
})

/**
 * Stop containers of a given mining node (ie. tari, merged).
 * It doesn't stop common containers, like Tor, Wallet, and BaseNode.
 * @prop {{ node: MiningNodeType }} node - the mining node, ie. 'tari'
 * @returns {Promise<void>}
 */
export const stopMiningNode = createAsyncThunk<
  void,
  {
    node: MiningNodeType
  },
  { state: RootState }
>('mining/stopNode', async ({ node }, thunkApi) => {
  try {
    const promises = []

    switch (node) {
      case 'tari':
        promises.push(
          thunkApi.dispatch(containersActions.stopByType(Container.SHA3Miner)),
        )
        break
      case 'merged':
        promises.push(
          thunkApi.dispatch(containersActions.stopByType(Container.MMProxy)),
        )
        promises.push(
          thunkApi.dispatch(containersActions.stopByType(Container.XMrig)),
        )
        promises.push(
          thunkApi.dispatch(containersActions.stopByType(Container.Monerod)),
        )
        break
    }

    thunkApi.dispatch(miningActions.stopSession({ node }))

    await Promise.all(promises)
  } catch (e) {
    return thunkApi.rejectWithValue(e)
  }
})
