import { createSlice } from '@reduxjs/toolkit'

import { ThemeType } from '../../styles/themes/types'
import { Schedule } from '../../types/general'
import { startOfUTCDay } from '../../utils/Date'

import { AppState, ExpertViewType, ViewType } from './types'

export const appInitialState: AppState = {
  expertView: 'hidden',
  expertSwitchDisabled: false,
  view: 'MINING',
  theme: 'light',
  schedules: {},
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
} = appSlice.actions

const reducer = appSlice.reducer
export default reducer
