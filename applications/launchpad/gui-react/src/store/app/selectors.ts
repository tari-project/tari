import { createSelector } from '@reduxjs/toolkit'

import { RootState } from '..'
import themes from '../../styles/themes'
import { Schedule, MiningNodeType } from '../../types/general'
import {
  selectTariSetupRequired,
  selectMergedSetupRequired,
} from '../mining/selectors'

export const selectExpertView = ({ app }: RootState) => app.expertView

export const selectExpertSwitchDisabled = ({ app }: RootState) =>
  app.expertSwitchDisabled

export const selectView = ({ app }: RootState) => app.view

export const selectTheme = ({ app }: RootState) => app.theme

export const selectThemeConfig = ({ app }: RootState) => {
  return themes[app.theme]
}

const selectSchedulesObject = (state: RootState) => state.app.schedules
export const selectSchedules = createSelector(
  selectSchedulesObject,
  schedules =>
    Object.values(schedules).map(schedule => {
      const { date, ...rest } = schedule

      return {
        ...rest,
        date: date && new Date(date),
      } as Schedule
    }),
)

export const selectSchedule =
  (scheduleId: string) =>
  ({ app }: RootState) => {
    const selectedSchedule = app.schedules[scheduleId]
    if (!selectedSchedule) {
      return undefined
    }

    const { date, ...rest } = selectedSchedule

    return {
      ...rest,
      date: date && new Date(date),
    } as Schedule
  }

export const selectActiveMiningTypes = createSelector(
  selectTariSetupRequired,
  selectMergedSetupRequired,
  (tariSetupRequired, mergedSetupRequired) => {
    const active = [] as MiningNodeType[]

    if (!tariSetupRequired) {
      active.push('tari')
    }

    if (!mergedSetupRequired) {
      active.push('merged')
    }

    return active
  },
)

export const selectOnboardingComplete = ({ app }: RootState) =>
  app.onboardingComplete

export const selectOnboardingCheckpoint = ({ app }: RootState) =>
  app.onboardingCheckpoint
