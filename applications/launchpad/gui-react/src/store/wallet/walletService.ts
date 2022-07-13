import { invoke } from '@tauri-apps/api/tauri'

import { toT } from '../../utils/Format'

export interface WalletIdentityDto {
  publicKey: string
  nodeId: string
  publicAddress: string
  emojiId: string
}
export const getIdentity: () => Promise<WalletIdentityDto> = () =>
  invoke<WalletIdentityDto>('wallet_identity')

export interface WalletBalanceDto {
  availableBalance: number
  pendingIncomingBalance: number
  pendingOutgoingBalance: number
}
export const getBalance: () => Promise<WalletBalanceDto> = async () => {
  const microTariBalance = await invoke<WalletBalanceDto>('wallet_balance')

  return {
    availableBalance: toT(microTariBalance.availableBalance),
    pendingIncomingBalance: toT(microTariBalance.pendingIncomingBalance),
    pendingOutgoingBalance: toT(microTariBalance.pendingIncomingBalance),
  }
}
