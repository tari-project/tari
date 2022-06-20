import { createAsyncThunk } from '@reduxjs/toolkit'

type WalletPassword = string

// TODO backend communication
export const unlockWallet = createAsyncThunk<
  {
    address: string
    tari: { balance: number; available: number }
  },
  WalletPassword
>('wallet/unlock', async _password => {
  await new Promise(resolve => setTimeout(resolve, 2000))

  return {
    address: '7a6ffed9-4252-427e-af7d-3dcaaf2db2df',
    tari: {
      balance: 11350057,
      available: 11349009,
    },
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
