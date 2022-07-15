import { createAsyncThunk } from '@reduxjs/toolkit'
import { cacheDir, sep } from '@tauri-apps/api/path'

import { RootState } from '..'
import { actions as baseNodeActions } from '../baseNode'
import { actions as miningActions } from '../mining'
import { actions as containersActions } from '../containers'
import { actions as dockerImagesActions } from '../dockerImages'
import { actions as credentialsActions } from '../credentials'

import { SettingsInputs } from '../../containers/SettingsContainer/types'

import MiningConfig from '../../config/mining'
import { InitialSettings } from './types'

const getSettings = async (): Promise<InitialSettings> => {
  const newCacheDir = await cacheDir()
  const network = 'dibbler'
  return {
    parole: '',
    moneroMiningAddress: 'test1',
    moneroWalletAddress: 'test2',
    numMiningThreads: 1,
    tariNetwork: network,
    cacheDir: newCacheDir,
    dockerRegistry: 'quay.io/tarilabs',
    dockerTag: 'latest',
    monerodUrl: MiningConfig.defaultMoneroUrls?.join(',') || '',
    moneroUseAuth: false,
    moneroUsername: '',
    moneroPassword: '',
    rootFolder: newCacheDir + 'tari' + sep + 'tmp' + sep + network,
  }
}

export const loadDefaultServiceSettings = createAsyncThunk<
  InitialSettings,
  void
>('service/start', async (_, thunkApi) => {
  const settings = await getSettings()
  const rootState = thunkApi.getState() as RootState
  if (!rootState.baseNode.rootFolder) {
    thunkApi.dispatch(baseNodeActions.setRootFolder(settings.rootFolder))
  }
  return settings
})

export const saveSettings = createAsyncThunk<
  void,
  { newSettings: SettingsInputs },
  { state: RootState }
>('settings/save', async ({ newSettings }, thunkApi) => {
  const { dispatch } = thunkApi
  // Set BaseNode config

  if ('baseNode' in newSettings && newSettings.baseNode) {
    if (newSettings.baseNode.network) {
      dispatch(baseNodeActions.setTariNetwork(newSettings.baseNode.network))
    }
    if (newSettings.baseNode.rootFolder) {
      dispatch(baseNodeActions.setRootFolder(newSettings.baseNode.rootFolder))
    }
  }

  // Set Mining config
  if ('mining' in newSettings && 'merged' in newSettings.mining) {
    const useAuth = newSettings.mining.merged.authentication

    dispatch(
      miningActions.setMergedConfig({
        ...newSettings.mining.merged,
        useAuth: Boolean(useAuth),
      }),
    )

    dispatch(
      credentialsActions.setMoneroCredentials({
        username: useAuth?.username || '',
        password: useAuth?.password || '',
      }),
    )
  }

  if ('docker' in newSettings) {
    dispatch({ type: 'settings/save', payload: { docker: newSettings.docker } })
    await dispatch(dockerImagesActions.getDockerImageList()).unwrap()
  }

  // Restart containers
  await dispatch(containersActions.restart())
})
