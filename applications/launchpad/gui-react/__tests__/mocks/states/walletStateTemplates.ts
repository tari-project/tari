import { WalletState } from '../../../src/store/wallet/types'

export const initialWallet: WalletState = {
  running: false,
  pending: false,
  address: '',
  unlocked: false,
  tari: {
    balance: 0,
    available: 0,
  },
}

export const unlockedWallet: WalletState = {
  running: false,
  pending: false,
  address: '',
  unlocked: true,
  tari: {
    balance: 0,
    available: 0,
  },
}

export const runningWallet = (
  address = 'the-wallet-address',
  tari = { balance: 0, available: 0 },
): WalletState => ({
  running: true,
  pending: false,
  address,
  unlocked: true,
  tari,
})
