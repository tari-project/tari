import { createAsyncThunk } from '@reduxjs/toolkit'

type WalletPassword = string

// TODO backend communication
export const unlockWallet = createAsyncThunk<
  {
    address: string
    tari: { balance: number; available: number }
  },
  WalletPassword
>('wallet/unlock', async password => {
  console.log('thunk')
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
