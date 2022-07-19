import { selectServiceSettings } from './../settings/selectors'
import { createAsyncThunk } from '@reduxjs/toolkit'
import { invoke } from '@tauri-apps/api/tauri'

import { RootState } from '../'
import { DockerImage, ServiceRecipe } from '../../types/general'

export const getDockerImageList = createAsyncThunk<
  { images: DockerImage[]; recipes: ServiceRecipe[] },
  void,
  { state: RootState }
>('dockerImages/getDockerImageList', async (_, thunkApi) => {
  try {
    const state = thunkApi.getState()
    const settings = selectServiceSettings(state)

    const result = await invoke<{
      imageInfo: DockerImage[]
      serviceRecipes: ServiceRecipe[]
    }>('image_info', {
      settings,
    })

    /**
     *  @TODO remove `updated: false` after the backend starts returning correct value
     */
    return {
      images: result.imageInfo.map(img => ({ ...img, updated: false })),
      recipes: result.serviceRecipes,
    }
  } catch (e) {
    return thunkApi.rejectWithValue(e)
  }
})

export const pullImages = createAsyncThunk<void, void, { state: RootState }>(
  'dockerImages/pullImages',
  async (_, thunkApi) => {
    thunkApi.getState().dockerImages.images.map(image => {
      thunkApi.dispatch(pullImage({ dockerImage: image.containerName }))
    })
  },
)

export const pullImage = createAsyncThunk<
  { dockerImage: string },
  { dockerImage: string },
  { state: RootState }
>('dockerImages/pullImage', async ({ dockerImage }, thunkApi) => {
  try {
    invoke('pull_image', { imageName: dockerImage })

    return { dockerImage }
  } catch (e) {
    return thunkApi.rejectWithValue(e)
  }
})
