import { Schedule } from '../types/general'
import { startOfUTCDay, dateInside } from '../utils/Date'

import { StartStop } from './types'

const getDaysBetween = (from: Date, to: Date) => {
  const days: Date[] = []
  let currentDay = startOfUTCDay(from)

  while (
    startOfUTCDay(currentDay).getTime() <
    startOfUTCDay(to).getTime() + 1
  ) {
    days.push(new Date(currentDay))

    currentDay = new Date(currentDay.setUTCDate(currentDay.getUTCDate() + 1))
  }

  return days
}

/**
 * @name getStartsStops
 * @description function that calculates mining start and stops in the given period, based on given schedules
 * if a scheduled period starts before `from` and does not finish before it, the mining is considered to start on `from`
 * every start/stop pertains to single mining node
 * if multiple schedules for the same mining node overlap, they are combined and single start/stop period is returned for them
 * returned start/stops are ordered by `start` properties
 *
 * @prop {Date} from - start of calculation
 * @prop {Date} to - end of calculation
 * @prop {Schedule[]} schedules - user-defined schedules used for mining start/stop calculation
 * @returns {StartStop[]}
 *
 * @typedef StartStop
 * @prop {MiningNodeType} toMine - type of mining that should be run
 * @prop {Date} start - when mining should start
 * @prop {Date} stop - when mining should stop
 */
export const getStartsStops = ({
  from,
  to,
  schedules,
}: {
  from: Date
  to: Date
  schedules: Schedule[]
}): StartStop[] => {
  const enabledSchedulesWithDates = schedules.filter(
    schedule => schedule.date && schedule.enabled,
  )

  const days = getDaysBetween(from, to)
  const recurringSchedules = schedules.filter(
    schedule => schedule.enabled && !schedule.date && schedule.days,
  )

  const schedulesGeneratedFromDays = days.flatMap(day => {
    const recurringSchedulesThisDay = recurringSchedules.filter(schedule =>
      // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
      schedule.days!.includes(day.getDay()),
    )

    return recurringSchedulesThisDay.map(recurring => ({
      ...recurring,
      date: day,
    }))
  })

  return [...enabledSchedulesWithDates, ...schedulesGeneratedFromDays]
    .filter(schedule => {
      // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
      const scheduleStart = startOfUTCDay(new Date(schedule.date!))
      scheduleStart.setUTCHours(schedule.interval.from.hours)
      scheduleStart.setUTCMinutes(schedule.interval.from.minutes)

      // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
      const scheduleStop = startOfUTCDay(new Date(schedule.date!))
      scheduleStop.setUTCHours(schedule.interval.to.hours)
      scheduleStop.setUTCMinutes(schedule.interval.to.minutes)

      return (
        dateInside(scheduleStart, { from, to }) ||
        dateInside(scheduleStop, { from, to })
      )
    })
    .flatMap(schedule =>
      schedule.type.map(miningType => {
        // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
        const startTime = startOfUTCDay(new Date(schedule.date!))
        startTime.setUTCHours(schedule.interval.from.hours)
        startTime.setUTCMinutes(schedule.interval.from.minutes)

        // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
        const stopTime = startOfUTCDay(new Date(schedule.date!))
        stopTime.setUTCHours(schedule.interval.to.hours)
        stopTime.setUTCMinutes(schedule.interval.to.minutes)

        return {
          start: startTime.getTime() < from.getTime() ? from : startTime,
          stop: stopTime,
          toMine: miningType,
          scheduleId: schedule.id,
        }
      }),
    )
    .sort((a, b) => a.start.getTime() - b.start.getTime())
    .reduce((accu, current) => {
      if (!accu.length) {
        return [current]
      }

      const lastOfType = [...accu]
        .reverse()
        .find(accuEl => accuEl.toMine === current.toMine)
      if (!lastOfType) {
        return [...accu, current]
      }

      const currentStartsDuringLast = dateInside(current.start, {
        from: lastOfType.start,
        to: lastOfType.stop,
      })
      const currentEndsAfterLastEnds =
        current.stop.getTime() > lastOfType.stop.getTime()
      if (currentStartsDuringLast && currentEndsAfterLastEnds) {
        lastOfType.stop = current.stop
        return accu
      } else if (currentStartsDuringLast && !currentEndsAfterLastEnds) {
        return accu
      }

      return [...accu, current]
    }, [] as StartStop[])
}
