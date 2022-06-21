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
  const microTariBalance = await invoke<{
    available_balance: number
    pending_incoming_balance: number
    pending_outgoing_balance: number
  }>('wallet_balance')

  return {
    availableBalance: microTariBalance.available_balance / 1000000,
    pendingIncomingBalance: microTariBalance.pending_incoming_balance / 1000000,
    pendingOutgoingBalance: microTariBalance.pending_outgoing_balance / 1000000,
  }
}
