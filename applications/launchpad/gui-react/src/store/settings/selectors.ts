import { RootState } from '../'
import { createSelector } from '@reduxjs/toolkit'
import {
  selectMergedAuthentication,
  selectMergedMiningAddress,
  selectMergedMiningThreads,
  selectMoneroUrls,
} from '../mining/selectors'
import {
  selectWallet,
  selectMoneroUsername,
  selectMoneroPassword,
} from '../credentials/selectors'
import { selectNetwork, selectRootFolder } from '../baseNode/selectors'
import { ServiceSettingsState } from './types'
import { Network } from '../../containers/BaseNodeContainer/types'

const isAuthActive = (auth?: { username?: string; password?: string }) => {
  return Boolean(auth?.username || auth?.password)
}

export const selectSettingsOpen = (state: RootState) => state.settings.open
export const selectActiveSettings = (state: RootState) => state.settings.which
export const selectSettingsState = (state: RootState) =>
  state.settings.serviceSettings

export const selectServiceSettings = createSelector(
  selectSettingsState,
  selectNetwork,
  selectRootFolder,
  selectWallet,
  selectMoneroUsername,
  selectMoneroPassword,
  selectMergedMiningThreads,
  selectMoneroUrls,
  selectMergedMiningAddress,
  selectMergedAuthentication,
  (
    serviceSettings: ServiceSettingsState,
    network: Network,
    rootFolder: string,
    parole: string,
    moneroUsername: string,
    moneroPassword: string,
    threads: number,
    moneroUrls: string,
    mergedMiningAddress?: string,
    mergedAuthentication?: {
      username?: string | undefined
      password?: string | undefined
    },
  ) => ({
    ...serviceSettings,
    tariNetwork: network,
    numMiningThreads: threads,
    moneroMiningAddress: mergedMiningAddress,
    monerodUrl: moneroUrls,
    moneroUseAuth: isAuthActive(mergedAuthentication),
    parole,
    walletPassword: parole,
    moneroUsername: moneroUsername,
    moneroPassword: moneroPassword,
    rootFolder,
  }),
)
