import { createAsyncThunk } from '@reduxjs/toolkit'
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/api/notification'

import { MiningNodeType, ScheduleId, CoinType } from '../../types/general'
import { selectContainerStatus } from '../containers/selectors'
import { actions as containersActions } from '../containers'
import { Container } from '../containers/types'
import t from '../../locales'
import { RootState } from '..'

import { actions as miningActions } from '.'
import { MiningActionReason } from './types'
import { selectTariSetupRequired, selectMergedSetupRequired } from './selectors'

const checkSetup: Record<MiningNodeType, (state: RootState) => void> = {
  tari: state => {
    const setupRequired = selectTariSetupRequired(state)

    if (setupRequired) {
      throw setupRequired
    }
  },
  merged: state => {
    const setupRequired = selectMergedSetupRequired(state)

    if (setupRequired) {
      throw setupRequired
    }
  },
}

/**
 * Start given mining node. It spawns all dependencies if needed.
 * @prop {NodeType} node - the node name, ie. 'tari', 'merged'
 * @returns {Promise<void>}
 */
export const startMiningNode = createAsyncThunk<
  void,
  { node: MiningNodeType; reason: MiningActionReason; schedule?: ScheduleId },
  { state: RootState }
>('mining/startNode', async ({ node, reason, schedule }, thunkApi) => {
  try {
    const rootState = thunkApi.getState()

    checkSetup[node](rootState)

    const miningSession = rootState.mining[node].session
    const scheduledMiningWasStoppedManually =
      miningSession?.finishedAt &&
      miningSession?.reason === MiningActionReason.Manual &&
      miningSession?.schedule
    if (
      scheduledMiningWasStoppedManually &&
      miningSession.schedule === schedule
    ) {
      return
    }

    const torStatus = selectContainerStatus(Container.Tor)(rootState)
    const baseNodeStatus = selectContainerStatus(Container.BaseNode)(rootState)
    const walletStatus = selectContainerStatus(Container.Wallet)(rootState)

    if (!torStatus.running && !torStatus.pending) {
      await thunkApi
        .dispatch(containersActions.start({ container: Container.Tor }))
        .unwrap()
    }

    if (!baseNodeStatus.running && !baseNodeStatus.pending) {
      await thunkApi
        .dispatch(containersActions.start({ container: Container.BaseNode }))
        .unwrap()
    }

    if (!walletStatus.running && !walletStatus.pending) {
      await thunkApi
        .dispatch(containersActions.start({ container: Container.Wallet }))
        .unwrap()
    }

    if (node === 'tari') {
      const minerStatus = selectContainerStatus(Container.SHA3Miner)(rootState)
      if (!minerStatus.running && !minerStatus.pending) {
        await thunkApi
          .dispatch(containersActions.start({ container: Container.SHA3Miner }))
          .unwrap()
        thunkApi.dispatch(
          miningActions.startNewSession({ node, reason, schedule }),
        )
      }
    }

    switch (node) {
      case 'merged':
        await thunkApi
          .dispatch(containersActions.start({ container: Container.MMProxy }))
          .unwrap()
        await thunkApi
          .dispatch(containersActions.start({ container: Container.XMrig }))
          .unwrap()
        thunkApi.dispatch(
          miningActions.startNewSession({ node, reason, schedule }),
        )
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
    reason: MiningActionReason
  },
  { state: RootState }
>('mining/stopNode', async ({ node, reason }, thunkApi) => {
  try {
    const promises = []

    const { getState } = thunkApi
    const state = getState()

    const miningSession = state.mining[node].session
    if (
      reason === MiningActionReason.Schedule &&
      miningSession?.startedAt &&
      !miningSession?.finishedAt &&
      miningSession?.reason === MiningActionReason.Manual
    ) {
      return
    }

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

    thunkApi.dispatch(miningActions.stopSession({ node, reason }))

    await Promise.all(promises)
  } catch (e) {
    return thunkApi.rejectWithValue(e)
  }
})

export const notifyUserAboutMinedTariBlock = createAsyncThunk<
  { message: string; header: string },
  { amount: number; currency: CoinType }
>('mining/notifyUser', async () => {
  const notification = {
    message:
      t.mining.notification.messages[
        Math.floor(Math.random() * t.mining.notification.messages.length)
      ],
    header:
      t.mining.notification.headers[
        Math.floor(Math.random() * t.mining.notification.headers.length)
      ],
  }

  const notifyAndIgnorePromise = async () => {
    const notify = () =>
      sendNotification({
        title: notification.header,
        body: notification.message,
      })

    if (await isPermissionGranted()) {
      notify()
      return
    }

    const perm = await requestPermission()
    if (perm === 'granted') {
      notify()
    }
  }
  notifyAndIgnorePromise()

  return notification
})

export const addMinedTx = createAsyncThunk<
  {
    amount: number
    node: MiningNodeType
    txId: string
  },
  {
    amount: number
    node: MiningNodeType
    txId: string
  },
  { state: RootState }
>('mining/addMinedTx', ({ amount, node, txId }, thunkApi) => {
  const rootState: RootState = thunkApi.getState()

  const session = rootState.mining[node].session

  if (
    !session ||
    (Boolean(session.history) && session.history.find(t => t.txId === txId))
  ) {
    return thunkApi.rejectWithValue({ amount, node, txId })
  }

  /**
   * @TODO - replace hard-coded currency after the app handles both currencies. (#298)
   */
  thunkApi.dispatch(
    notifyUserAboutMinedTariBlock({
      amount,
      currency: 'xtr',
    }),
  )

  return {
    amount,
    node,
    txId,
  }
})
