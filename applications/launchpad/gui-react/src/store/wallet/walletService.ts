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
export const getBalance: () => Promise<WalletBalanceDto> = async () => {
  const microTariBalance = await invoke<WalletBalanceDto>('wallet_balance')

  return {
    availableBalance: microTariBalance.availableBalance / 1000000,
    pendingIncomingBalance: microTariBalance.pendingIncomingBalance / 1000000,
    pendingOutgoingBalance: microTariBalance.pendingIncomingBalance / 1000000,
  }
}
