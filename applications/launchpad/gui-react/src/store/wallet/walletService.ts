import { invoke } from '@tauri-apps/api/tauri'

import { toT } from '../../utils/Format'

interface WalletIdentityDto {
  publicAddress: string
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
    availableBalance: toT(microTariBalance.availableBalance),
    pendingIncomingBalance: toT(microTariBalance.pendingIncomingBalance),
    pendingOutgoingBalance: toT(microTariBalance.pendingIncomingBalance),
  }
}
