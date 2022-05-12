import { createSlice } from '@reduxjs/toolkit'
import { ThemeType } from '../../styles/themes/types'

import { AppState, ExpertViewType, ViewType } from './types'

const appInitialState: AppState = {
  expertView: 'hidden',
  view: 'MINING',
  theme: 'light',
  schedules: [
    {
      id: 'asdf',
      enabled: true,
      days: [0, 1, 2],
      interval: {
        from: { hours: 3, minutes: 0 },
        to: { hours: 19, minutes: 35 },
      },
      type: ['merged'],
    },
    {
      id: 'qwer',
      enabled: false,
      days: [4, 5],
      interval: {
        from: { hours: 7, minutes: 0 },
        to: { hours: 15, minutes: 0 },
      },
      type: ['merged', 'tari'],
    },
    {
      id: 'qwer1',
      enabled: true,
      date: new Date('2022-05-14'),
      interval: {
        from: { hours: 7, minutes: 0 },
        to: { hours: 15, minutes: 0 },
      },
      type: ['merged', 'tari'],
    },
    {
      id: 'qwer3',
      enabled: false,
      date: new Date('2022-05-14'),
      interval: {
        from: { hours: 7, minutes: 0 },
        to: { hours: 15, minutes: 0 },
      },
      type: ['merged', 'tari'],
    },
  ],
}

const appSlice = createSlice({
  name: 'app',
  initialState: appInitialState,
  reducers: {
    setExpertView(state, { payload }: { payload: ExpertViewType }) {
      state.expertView = payload
    },
    setTheme(state, { payload }: { payload: ThemeType }) {
      state.theme = payload
    },
    setPage(state, { payload }: { payload: ViewType }) {
      state.view = payload
    },
  },
})

export const { setExpertView, setTheme, setPage } = appSlice.actions

const reducer = appSlice.reducer
export default reducer
