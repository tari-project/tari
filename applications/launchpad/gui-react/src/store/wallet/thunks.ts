import { createAsyncThunk } from '@reduxjs/toolkit'

import { RootState } from '..'
import { Container } from '../containers/types'
import {
  selectContainerStatus,
  selectRunningContainers,
} from '../containers/selectors'
import { actions as containersActions } from '../containers'

import * as walletService from './walletService'
import { selectContainerStatuses } from './selectors'

type WalletPassword = string

// TODO backend communication
const waitForWalletToBeResponsive = () =>
  new Promise(resolve => setTimeout(resolve, 200))

export const unlockWallet = createAsyncThunk<
  {
    address: string
    tari: { balance: number; available: number }
  },
  WalletPassword,
  { state: RootState }
>('wallet/unlock', async (walletPassword, thunkApi) => {
  try {
    const rootState = thunkApi.getState()
    const torStatus = selectContainerStatus(Container.Tor)(rootState)

    if (!torStatus.running && !torStatus.pending) {
      await thunkApi
        .dispatch(
          containersActions.start({
            service: Container.Tor,
          }),
        )
        .unwrap()
    }

    const walletStatus = selectContainerStatus(Container.Wallet)(rootState)
    if (!walletStatus.running && !walletStatus.pending) {
      await thunkApi
        .dispatch(
          containersActions.start({
            service: Container.Wallet,
            serviceSettings: { walletPassword },
          }),
        )
        .unwrap()
    }

    await waitForWalletToBeResponsive()

    const getWalletIdentityPromise = walletService.getIdentity()
    const getBalancePromise = walletService.getBalance()

    const [walletIdentity, tari] = await Promise.all([
      getWalletIdentityPromise,
      getBalancePromise,
    ])

    return {
      address: walletIdentity.publicAddress,
      tari: {
        balance:
          tari.availableBalance -
          tari.pendingOutgoingBalance +
          tari.pendingIncomingBalance,
        available: tari.availableBalance,
      },
    }
  } catch (e) {
    return thunkApi.rejectWithValue(e)
  }
})

export const start = unlockWallet

export const stop = createAsyncThunk<void, void, { state: RootState }>(
  'wallet/stop',
  async (_, thunkApi) => {
    try {
      const rootState = thunkApi.getState()
      const [torContainerStatus, walletContainerStatus] =
        selectContainerStatuses(rootState)

      thunkApi.dispatch(containersActions.stop(walletContainerStatus.id))

      const runningContainers = selectRunningContainers(rootState)
      const otherServicesRunning = runningContainers.some(
        rc => rc !== Container.Tor && rc !== Container.Wallet,
      )
      if (!otherServicesRunning) {
        thunkApi.dispatch(containersActions.stop(torContainerStatus.id))
      }
    } catch (e) {
      return thunkApi.rejectWithValue(e)
    }
  },
)

export const updateWalletBalance = createAsyncThunk<{
  tari: { balance: number; available: number }
}>('wallet/updateBalance', async (_, thunkApi) => {
  let timer
  try {
    // do not flicker loading indicator if backend call is sub 300ms
    timer = setTimeout(() => {
      thunkApi.dispatch({ type: 'wallet/tariBalancePending' })
    }, 300)
    const tari = await walletService.getBalance()

    return {
      tari: {
        balance:
          tari.availableBalance -
          tari.pendingOutgoingBalance +
          tari.pendingIncomingBalance,
        available: tari.availableBalance,
      },
    }
  } catch (e) {
    return thunkApi.rejectWithValue(e)
  } finally {
    if (timer) {
      clearTimeout(timer)
    }
  }
})
