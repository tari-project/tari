import { createSlice } from '@reduxjs/toolkit'

import { DockerImagesState } from './types'
import { getDockerImageList, pullImage, pullImages } from './thunks'

import t from '../../locales'
import { DockerImage } from '../../types/general'

import AppConfig from '../../config/app'

export const initialState: DockerImagesState = {
  loaded: false,
  images: [],
  recipes: {},
  downloadWithTBot: [],
  dismissedDownloads: [],
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
          progress?: string
          status?: string
        }
      },
    ) {
      const imageIdx = state.images.findIndex(
        img => img.dockerImage === payload.dockerImage,
      )

      if (imageIdx < 0) {
        return
      }

      const isCompleted = Boolean(
        payload.status?.toLowerCase().includes('image is up to date') ||
          payload.status?.toLowerCase().includes('downloaded newer image'),
      )

      const isError = payload.error

      state.images[imageIdx].updated = isCompleted
      state.images[imageIdx].pending = !isCompleted && !isError
      state.images[imageIdx].error = isError || state.images[imageIdx].error
      state.images[imageIdx].progress =
        payload.progress === undefined
          ? state.images[imageIdx].progress
          : payload.progress
      state.images[imageIdx].status =
        payload.status || state.images[imageIdx].status
    },
    pushToTBotQueue(state, { payload }: { payload: DockerImage }) {
      if (
        state.dismissedDownloads?.find(
          t =>
            t.containerName === payload.containerName &&
            new Date().getTime() - t.dismissedAt <
              AppConfig.dockerDownloadDismissValidFor,
        )
      ) {
        return
      }

      if (!state.downloadWithTBot) {
        state.downloadWithTBot = [payload]
      }

      if (
        state.downloadWithTBot.find(
          i => i.containerName === payload.containerName,
        )
      ) {
        return
      }

      state.downloadWithTBot = state.downloadWithTBot.concat([payload])
    },
    removeFromTBotQueue(
      state,
      { payload }: { payload: { image: DockerImage; dismiss: boolean } },
    ) {
      if (!state.downloadWithTBot || state.downloadWithTBot.length === 0) {
        return
      }

      if (payload.dismiss) {
        const dIdx = state.dismissedDownloads
          ? state.dismissedDownloads?.findIndex(
              i => i.containerName === payload.image.containerName,
            )
          : -1
        if (dIdx > -1) {
          state.dismissedDownloads[dIdx].dismissedAt = new Date().getTime()
        } else {
          if (!state.dismissedDownloads) {
            state.dismissedDownloads = [
              {
                containerName: payload.image.containerName,
                dismissedAt: new Date().getTime(),
              },
            ]
          } else {
            state.dismissedDownloads = state.dismissedDownloads.concat([
              {
                containerName: payload.image.containerName,
                dismissedAt: new Date().getTime(),
              },
            ])
          }
        }
      }

      state.downloadWithTBot = state.downloadWithTBot.filter(
        i => i.containerName !== payload.image.containerName,
      )
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
    builder.addCase(pullImage.pending, (state, payload) => {
      const imageIdx = state.images.findIndex(
        img => img.containerName === payload.meta.arg.dockerImage,
      )

      if (imageIdx > -1) {
        state.images[imageIdx].pending = true
      }
    })
    builder.addCase(pullImage.fulfilled, (state, action) => {
      const imageIdx = state.images.findIndex(
        img => img.containerName === action.payload.dockerImage,
      )

      if (imageIdx > -1) {
        state.images[imageIdx].pending = true
        state.images[imageIdx].updated = false
        state.images[imageIdx].error = undefined
        state.images[imageIdx].status = 'Starting...'
        state.images[imageIdx].progress = ''
      }
    })
    builder.addCase(pullImage.rejected, (state, action) => {
      const dockerContainer = action.meta.arg.dockerImage

      const imageIdx = state.images.findIndex(
        img => img.containerName === dockerContainer,
      )

      if (imageIdx > -1) {
        state.images[imageIdx].pending = false
        state.images[imageIdx].updated = false
        state.images[imageIdx].error =
          (action.payload as Error | undefined)?.toString() ||
          t.common.phrases.somethingWentWrong
        state.images[imageIdx].status = 'Error'
        state.images[imageIdx].progress = ''
      }
    })
  },
})

export const actions = {
  ...slice.actions,
  getDockerImageList,
  pullImage,
  pullImages,
}

const reducer = slice.reducer
export default reducer
