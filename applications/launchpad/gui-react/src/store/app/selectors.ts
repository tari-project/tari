import { createSelector } from '@reduxjs/toolkit'
import { RootState } from '..'
import themes from '../../styles/themes'
import { Schedule } from '../../types/general'

export const selectExpertView = ({ app }: RootState) => app.expertView

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
