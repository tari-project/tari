import { createAsyncThunk } from '@reduxjs/toolkit'
import { invoke } from '@tauri-apps/api/tauri'

import { RootState } from '../'
import { DockerImage, DockerImagePullStatus } from '../../types/general'

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
    return images.map(img => ({ ...img, latest: false }))
  } catch (e) {
    return thunkApi.rejectWithValue(e)
  }
})

export const pullImage = createAsyncThunk<void, { dockerImage: string }>(
  'app/pullImage',
  async ({ dockerImage }, thunkApi) => {
    try {
      thunkApi.dispatch({
        type: 'app/setDockerProgress',
        payload: {
          dockerImage,
          status: DockerImagePullStatus.Waiting,
        },
      })
      await new Promise(resolve => setTimeout(resolve, 1000))
      thunkApi.dispatch({
        type: 'app/setDockerProgress',
        payload: {
          dockerImage,
          status: DockerImagePullStatus.Pulling,
          progress: 0,
        },
      })
      await new Promise(resolve => setTimeout(resolve, 1000))
      thunkApi.dispatch({
        type: 'app/setDockerProgress',
        payload: {
          dockerImage,
          progress: 20,
        },
      })
      await new Promise(resolve => setTimeout(resolve, 1000))
      thunkApi.dispatch({
        type: 'app/setDockerProgress',
        payload: {
          dockerImage,
          progress: 60,
        },
      })
      await new Promise(resolve => setTimeout(resolve, 1000))
      thunkApi.dispatch({
        type: 'app/setDockerProgress',
        payload: {
          dockerImage,
          progress: 80,
        },
      })
      await new Promise(resolve => setTimeout(resolve, 1000))
      thunkApi.dispatch({
        type: 'app/setDockerProgress',
        payload: {
          dockerImage,
          status: DockerImagePullStatus.Ready,
          progress: 100,
        },
      })
      // TODO pull image after https://github.com/Altalogy/tari/issues/311
    } catch (e) {
      return thunkApi.rejectWithValue(e)
    }
  },
)
