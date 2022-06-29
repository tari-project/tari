import { createSlice } from '@reduxjs/toolkit'

import { DockerImagePullStatus } from '../../types/general'

import { DockerImagesState } from './types'
import { getDockerImageList, pullImage } from './thunks'

export const initialState: DockerImagesState = {
  loaded: false,
  images: [],
  recipes: {},
}

const slice = createSlice({
  name: 'dockerImages',
  initialState,
  reducers: {
    setProgress(
      state,
      {
        payload,
      }: {
        payload: {
          dockerImage: string
          error?: string
          progress?: number
          status?: DockerImagePullStatus
        }
      },
    ) {
      const image = state.images.find(
        img => img.dockerImage === payload.dockerImage,
      )

      if (!image) {
        return
      }

      image.latest = payload.status === DockerImagePullStatus.Ready
      image.pending = payload.status !== DockerImagePullStatus.Ready
      image.error = payload.error || image.error
      image.progress =
        payload.progress === undefined ? image.progress : payload.progress
      image.status = payload.status || image.status
    },
  },
  extraReducers: builder => {
    builder.addCase(getDockerImageList.pending, state => {
      state.loaded = false
    })
    builder.addCase(getDockerImageList.fulfilled, (state, action) => {
      state.loaded = true
      const { images, recipes } = action.payload
      state.images = images
      state.recipes = recipes.reduce(
        (accu, current) => ({
          ...accu,
          [current[0]]: current,
        }),
        {},
      )
    })
  },
})

export const actions = {
  ...slice.actions,
  getDockerImageList,
  pullImage,
}

const reducer = slice.reducer
export default reducer
