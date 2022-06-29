import { createAsyncThunk } from '@reduxjs/toolkit'

import { RootState } from '..'
import { Container } from '../containers/types'
import { actions as containersActions } from '../containers'

import * as walletService from './walletService'

// TODO backend communication
const waitForWalletToBeResponsive = () =>
  new Promise(resolve => setTimeout(resolve, 200))

export const unlockWallet = createAsyncThunk<
  {
    address: string
    tari: { balance: number; available: number }
  },
  void,
  { state: RootState }
>('wallet/unlock', async (_, thunkApi) => {
  try {
    await thunkApi
      .dispatch(
        containersActions.startRecipe({
          containerName: Container.Wallet,
          // TODO service settings from state
          // serviceSettings: { walletPassword },
        }),
      )
      .unwrap()

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
  (_, thunkApi) =>
    thunkApi.dispatch(containersActions.stopRecipe(Container.Wallet)).unwrap(),
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
