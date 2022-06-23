import { createAsyncThunk } from '@reduxjs/toolkit'
import { invoke } from '@tauri-apps/api/tauri'

import { RootState } from '../'

import { DockerImage } from './types'

export const getDockerImageList = createAsyncThunk<
  DockerImage[],
  void,
  { state: RootState }
>('app/getDockerImageList', async (_, thunkApi) => {
  try {
    const state = thunkApi.getState()

    const images = await invoke<DockerImage[]>('image_list', {
      settings: state.settings.serviceSettings,
    })

    // TODO get status from backend after https://github.com/Altalogy/tari/issues/311
    return images.map(img => ({ ...img, latest: img.displayName !== 'Wallet' }))
  } catch (e) {
    return thunkApi.rejectWithValue(e)
  }
})

export const pullImage = createAsyncThunk<void, { dockerImage: string }>(
  'app/pullImage',
  async (_, thunkApi) => {
    try {
      // TODO pull image after https://github.com/Altalogy/tari/issues/311
    } catch (e) {
      return thunkApi.rejectWithValue(e)
    }
  },
)
