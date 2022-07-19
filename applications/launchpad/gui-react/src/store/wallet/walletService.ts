import { invoke } from '@tauri-apps/api/tauri'

import { toT } from '../../utils/Format'

export interface WalletIdentityDto {
  nodeId: string
  publicKey: number[]
  publicAddress: string
  emojiId: string
}

export const getIdentity: () => Promise<WalletIdentityDto> = () =>
  invoke<WalletIdentityDto>('wallet_identity')

export interface WalletBalanceDto {
  available_balance: number
  pending_incoming_balance: number
  pending_outgoing_balance: number
}

export interface WalletBalance {
  availableBalance: number
  pendingIncomingBalance: number
  pendingOutgoingBalance: number
}

export const getBalance: () => Promise<WalletBalance> = async () => {
  const microTariBalance = await invoke<WalletBalanceDto>('wallet_balance')

  return {
    availableBalance: toT(microTariBalance.available_balance),
    pendingIncomingBalance: toT(microTariBalance.pending_incoming_balance),
    pendingOutgoingBalance: toT(microTariBalance.pending_outgoing_balance),
  }
}
