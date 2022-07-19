import { WalletState } from '../../../src/store/wallet/types'

export const initialWallet: WalletState = {
  address: {
    uri: '',
    emoji: '',
  },
  unlocked: false,
  tari: {
    pending: false,
    balance: 0,
    available: 0,
  },
}

export const unlockedWallet: WalletState = {
  address: {
    uri: 'someWalletAddress',
    emoji: '',
  },
  unlocked: true,
  tari: {
    pending: false,
    balance: 0,
    available: 0,
  },
}

export const runningWallet = (
  address = 'the-wallet-address',
  tari = { balance: 0, available: 0, pending: false },
): WalletState => ({
  address: {
    uri: address,
    emoji: '',
  },
  unlocked: true,
  tari,
})
