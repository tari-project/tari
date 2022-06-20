import { invoke } from '@tauri-apps/api/tauri'

interface WalletIdentityDto {
  publicAddress: string
  emojiRepresentation: string
}
export const getIdentity: () => Promise<WalletIdentityDto> = () =>
  invoke('wallet_identity')

interface WalletBalanceDto {
  availableBalance: number
  pendingIncomingBalance: number
  pendingOutgoingBalance: number
}
export const getBalance: () => Promise<WalletBalanceDto> = () =>
  invoke('wallet_balance')
