import { createAsyncThunk } from '@reduxjs/toolkit'

import { RootState } from '..'
import { Container } from '../containers/types'
import { actions as containersActions } from '../containers'

import * as walletService from './walletService'
import { temporaryActions } from '../temporary'
import { convertU8ToString } from '../../utils/Format'

const getWalletDataWithPolling = async (): Promise<
  [walletService.WalletIdentityDto, walletService.WalletBalance]
> => {
  let watchdog = 0

  while (watchdog < 31) {
    try {
      const getWalletIdentityPromise = walletService.getIdentity()
      const getBalancePromise = walletService.getBalance()

      const walletData = await Promise.all([
        getWalletIdentityPromise,
        getBalancePromise,
      ])

      return walletData
    } catch (e) {
      if (watchdog === 30) {
        throw e
      }

      await new Promise(resolve => setTimeout(resolve, 500))
      watchdog++
    }
  }

  throw new Error('No wallet data after 30+ attempts')
}

export const unlockWallet = createAsyncThunk<
  {
    address: { uri: string; emoji: string; publicKey: string }
    tari: { balance: number; available: number }
  },
  void,
  { state: RootState }
>('wallet/unlock', async (_, thunkApi) => {
  try {
    thunkApi.dispatch(
      temporaryActions.setWalletPasswordConfirmation(
        'waiting_for_confirmation',
      ),
    )

    await thunkApi
      .dispatch(
        containersActions.startRecipe({
          containerName: Container.Wallet,
        }),
      )
      .unwrap()

    const [walletIdentity, tari] = await getWalletDataWithPolling()

    return {
      address: {
        uri: walletIdentity.publicAddress,
        emoji: walletIdentity.emojiId,
        publicKey: convertU8ToString(walletIdentity.publicKey.toString()),
      },
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
