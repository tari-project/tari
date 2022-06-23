import { createSlice } from '@reduxjs/toolkit'

import { ThemeType } from '../../styles/themes/types'
import { Schedule } from '../../types/general'
import { startOfUTCDay } from '../../utils/Date'

import {
  AppState,
  ExpertViewType,
  ViewType,
  DockerImagePullStatus,
} from './types'
import { getDockerImageList } from './thunks'

export const appInitialState: AppState = {
  expertView: 'hidden',
  expertSwitchDisabled: false,
  view: 'MINING',
  theme: 'light',
  schedules: {},
  onboardingComplete: false,
  dockerImages: {
    loaded: false,
    images: [],
  },
}

const appSlice = createSlice({
  name: 'app',
  initialState: appInitialState,
  reducers: {
    setExpertView(state, { payload }: { payload: ExpertViewType }) {
      state.expertView = payload
    },
    setExpertSwitchDisabled(state, { payload }: { payload: boolean }) {
      state.expertSwitchDisabled = payload
    },
    setTheme(state, { payload }: { payload: ThemeType }) {
      state.theme = payload
    },
    setPage(state, { payload }: { payload: ViewType }) {
      state.view = payload
    },
    toggleSchedule(state, { payload: scheduleId }: { payload: string }) {
      state.schedules[scheduleId].enabled = !state.schedules[scheduleId].enabled
    },
    removeSchedule(state, { payload: scheduleId }: { payload: string }) {
      delete state.schedules[scheduleId]
    },
    updateSchedule(
      state,
      {
        payload: { scheduleId, value },
      }: { payload: { scheduleId: string; value: Schedule } },
    ) {
      const { date, ...rest } = value
      const newSchedule = {
        ...state.schedules[scheduleId],
        ...rest,
      }

      newSchedule.date = date ? startOfUTCDay(date).toISOString() : undefined

      state.schedules[scheduleId] = newSchedule
    },
    setOnboardingComplete(state, { payload }: { payload: boolean }) {
      state.onboardingComplete = payload
    },
    setDockerProgress(
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
      const image = state.dockerImages.images.find(
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
      state.dockerImages.loaded = false
    })
    builder.addCase(getDockerImageList.fulfilled, (state, action) => {
      state.dockerImages.loaded = true
      state.dockerImages.images = action.payload
    })
  },
})

export const {
  setExpertView,
  setExpertSwitchDisabled,
  setTheme,
  setPage,
  toggleSchedule,
  removeSchedule,
  updateSchedule,
  setOnboardingComplete,
} = appSlice.actions

export * from './thunks'

const reducer = appSlice.reducer
export default reducer
