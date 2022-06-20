import { createAsyncThunk } from '@reduxjs/toolkit'

import { RootState } from '..'
import { Container } from '../containers/types'
import { selectContainerStatus } from '../containers/selectors'
import { actions as containersActions } from '../containers'

type WalletPassword = string

// TODO backend communication
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
            serviceSettings: { walletPassword },
          }),
        )
        .unwrap()
    }

    const walletStatus = selectContainerStatus(Container.Wallet)(rootState)
    if (!walletStatus.running && !walletStatus.pending) {
      await thunkApi
        .dispatch(containersActions.start({ service: Container.Wallet }))
        .unwrap()
    }

    await new Promise(resolve => setTimeout(resolve, 2000))

    return {
      address: '7a6ffed9-4252-427e-af7d-3dcaaf2db2df',
      tari: {
        balance: 11350057,
        available: 11349009,
      },
    }
  } catch (e) {
    return thunkApi.rejectWithValue(e)
  }
})

export const start = createAsyncThunk<void>('wallet/start', async () => {
  await new Promise(resolve => setTimeout(resolve, 2000))
})

export const stop = createAsyncThunk<void>('wallet/stop', async () => {
  await new Promise(resolve => setTimeout(resolve, 2000))
})

export const updateWalletBalance = createAsyncThunk<{
  tari: { balance: number; available: number }
}>('wallet/updateBalance', async (_, thunkApi) => {
  let timer
  try {
    // do not flicker loading indicator if backend call is sub 300ms
    timer = setTimeout(() => {
      thunkApi.dispatch({ type: 'wallet/tariBalancePending' })
    }, 300)
    await new Promise(resolve => setTimeout(resolve, 2000))

    return {
      tari: {
        balance: 11350058,
        available: 11350058,
      },
    }
  } finally {
    if (timer) {
      clearTimeout(timer)
    }
  }
})
