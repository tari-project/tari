import { createAsyncThunk } from '@reduxjs/toolkit'
import { cacheDir } from '@tauri-apps/api/path'

import { RootState } from '..'
import { actions as baseNodeActions } from '../baseNode'
import { actions as miningActions } from '../mining'
import { actions as containersActions } from '../containers'

import { SettingsInputs } from '../../containers/SettingsContainer/types'

import MiningConfig from '../../config/mining'

const getSettings = async () => ({
  walletPassword: 'tari',
  moneroMiningAddress: 'test1',
  moneroWalletAddress: 'test2',
  // '5AJ8FwQge4UjT9Gbj4zn7yYcnpVQzzkqr636pKto59jQcu85CFsuYVeFgbhUdRpiPjUCkA4sQtWApUzCyTMmSigFG2hDo48',
  numMiningThreads: 1,
  tariNetwork: 'dibbler',
  cacheDir: await cacheDir(),
  dockerRegistry: 'quay.io/tarilabs',
  dockerTag: 'latest',
  monerodUrl: MiningConfig.defaultMoneroUrls?.join(',') || '',
  moneroUseAuth: false,
  moneroUsername: '',
  moneroPassword: '',
})

export const loadDefaultServiceSettings = createAsyncThunk<unknown>(
  'service/start',
  getSettings,
)

export const saveSettings = createAsyncThunk<
  void,
  { newSettings: SettingsInputs },
  { state: RootState }
>('settings/save', async ({ newSettings }, thunkApi) => {
  const { dispatch } = thunkApi
  // Set BaseNode config
  if (
    'baseNode' in newSettings &&
    newSettings.baseNode &&
    newSettings.baseNode.network
  ) {
    dispatch(baseNodeActions.setTariNetwork(newSettings.baseNode.network))
  }

  // Set Mining config
  if ('mining' in newSettings && 'merged' in newSettings.mining) {
    dispatch(miningActions.setMergedConfig(newSettings.mining.merged))
  }

  // Restart containers
  await dispatch(containersActions.restart())
})
