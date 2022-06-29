import { RootState } from '../'
import { selectMoneroUrls } from '../mining/selectors'

const isAuthActive = (auth?: { username?: string; password?: string }) => {
  return Boolean(auth?.username || auth?.password)
}

export const selectSettingsOpen = (state: RootState) => state.settings.open
export const selectActiveSettings = (state: RootState) => state.settings.which
export const selectServiceSettings = (state: RootState) => ({
  ...state.settings.serviceSettings,
  tariNetwork: state.baseNode.network,
  numMiningThreads: state.mining.merged.threads,
  moneroMiningAddress: state.mining.merged.address,
  monerodUrl: selectMoneroUrls(state),
  moneroUseAuth: isAuthActive(state.mining.merged.authentication),
  moneroUsername: state.mining.merged.authentication?.username || '',
  moneroPassword: state.mining.merged.authentication?.password || '',
  rootFolder: state.baseNode.rootFolder,
})
