import { Schedule } from '../types/general'
import { clearTime, dateInside } from '../utils/Date'

import { StartStop } from './types'

const getDaysBetween = (from: Date, to: Date) => {
  const days: Date[] = []
  let currentDay = clearTime(from)

  while (clearTime(currentDay).getTime() < clearTime(to).getTime() + 1) {
    days.push(new Date(currentDay))

    currentDay = new Date(currentDay.setUTCDate(currentDay.getUTCDate() + 1))
  }

  return days
}

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
      schedule.days!.includes(day.getDay()),
    )

    return recurringSchedulesThisDay.map(recurring => ({
      ...recurring,
      date: day,
    }))
  })

  return [...enabledSchedulesWithDates, ...schedulesGeneratedFromDays]
    .filter(schedule => {
      const scheduleStart = clearTime(new Date(schedule.date!))
      scheduleStart.setUTCHours(schedule.interval.from.hours)
      scheduleStart.setUTCMinutes(schedule.interval.from.minutes)

      const scheduleStop = clearTime(new Date(schedule.date!))
      scheduleStop.setUTCHours(schedule.interval.to.hours)
      scheduleStop.setUTCMinutes(schedule.interval.to.minutes)

      return (
        dateInside(scheduleStart, { from, to }) ||
        dateInside(scheduleStop, { from, to })
      )
    })
    .flatMap(schedule =>
      schedule.type.map(miningType => {
        const startTime = clearTime(new Date(schedule.date!))
        startTime.setUTCHours(schedule.interval.from.hours)
        startTime.setUTCMinutes(schedule.interval.from.minutes)

        const stopTime = clearTime(new Date(schedule.date!))
        stopTime.setUTCHours(schedule.interval.to.hours)
        stopTime.setUTCMinutes(schedule.interval.to.minutes)

        return {
          start: startTime.getTime() < from.getTime() ? from : startTime,
          stop: stopTime,
          toMine: miningType,
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
