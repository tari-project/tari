import { createAsyncThunk } from '@reduxjs/toolkit'

type WalletPassword = string

export const unlockWallet = createAsyncThunk<
  {
    address: string
    tari: { balance: number; available: number }
  },
  WalletPassword
>('unlockWallet', async password => {
  console.log(`unlocking wallet with password ${password}`)
  await new Promise(resolve => setTimeout(resolve, 2000))

  return {
    address: 'your wallet balance',
    tari: {
      balance: 11350057,
      available: 11349009,
    },
  }
})
