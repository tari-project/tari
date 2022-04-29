import { createAsyncThunk } from '@reduxjs/toolkit'

type WalletPassword = string

// TODO backend communication
export const unlockWallet = createAsyncThunk<
  {
    address: string
    tari: { balance: number; available: number }
  },
  WalletPassword
>('unlockWallet', async password => {
  await new Promise(resolve => setTimeout(resolve, 2000))

  return {
    address: '7a6ffed9-4252-427e-af7d-3dcaaf2db2df',
    tari: {
      balance: 11350057,
      available: 11349009,
    },
  }
})
