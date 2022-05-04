import { createAsyncThunk } from '@reduxjs/toolkit'
import { cacheDir, sep } from '@tauri-apps/api/path'

const getSettings = async () => ({
  walletPassword: 'tari',
  moneroMiningAddress:
    '5AJ8FwQge4UjT9Gbj4zn7yYcnpVQzzkqr636pKto59jQcu85CFsuYVeFgbhUdRpiPjUCkA4sQtWApUzCyTMmSigFG2hDo48',
  numMiningThreads: 1,
  tariNetwork: 'dibbler',
  rootFolder: (await cacheDir()) + 'tari' + sep + 'tmp' + sep + 'dibbler',
  dockerRegistry: 'quay.io/tarilabs',
  dockerTag: 'latest',
  monerodUrl:
    'http://stagenet.community.xmr.to:38081,http://monero-stagenet.exan.tech:3808',
  moneroUseAuth: false,
  moneroUsername: '',
  moneroPassword: '',
})

export const loadDefaultServiceSettings = createAsyncThunk<unknown>(
  'service/start',
  getSettings,
)
