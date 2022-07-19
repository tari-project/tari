import { RootState } from '../'
import { createSelector } from '@reduxjs/toolkit'
import {
  selectMergedMiningAddress,
  selectMergedMiningThreads,
  selectMergedUseAuth,
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
  selectMergedUseAuth,
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
    mergedUseAuth?: boolean,
  ) => ({
    ...serviceSettings,
    tariNetwork: network,
    numMiningThreads: threads,
    moneroMiningAddress: mergedMiningAddress,
    monerodUrl: moneroUrls,
    moneroUseAuth: mergedUseAuth,
    parole,
    walletPassword: parole,
    moneroUsername: moneroUsername,
    moneroPassword: moneroPassword,
    rootFolder,
  }),
)
