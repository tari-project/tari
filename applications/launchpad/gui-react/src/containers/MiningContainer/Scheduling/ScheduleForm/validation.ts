import { Schedule, Interval } from '../../../../types/general'
import { startOfUTCDay } from '../../../../utils/Date'
import t from '../../../../locales'

const validateInterval = (interval: Interval): string | undefined => {
  if (interval.from.hours === interval.to.hours) {
    if (interval.from.minutes > interval.to.minutes) {
      return t.mining.scheduling.error_miningEndsBeforeItStarts
    }

    if (interval.from.minutes === interval.to.minutes) {
      return t.mining.scheduling.error_miningEndsWhenItStarts
    }
  }

  if (interval.from.hours > interval.to.hours) {
    return t.mining.scheduling.error_miningEndsBeforeItStarts
  }
}

const validateDate = (date?: Date): string | undefined => {
  if (!date) {
    return
  }

  if (startOfUTCDay(date) < startOfUTCDay(new Date())) {
    return t.mining.scheduling.error_miningInThePast
  }
}

export const validate = (schedule: Schedule): string | undefined => {
  const intervalError = validateInterval(schedule.interval)
  const dateError = validateDate(schedule.date)

  return intervalError || dateError
}
