import { Schedule, Interval } from '../../../../types/general'
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

export const validate = (schedule: Schedule): string | undefined => {
  const intervalError = validateInterval(schedule.interval)

  return intervalError
}
