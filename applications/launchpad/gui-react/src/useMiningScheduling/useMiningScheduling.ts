import { useEffect, useRef, useCallback, useState } from 'react'

import { startOfMinute } from '../utils/Date'
import { MiningNodeType, Schedule, ScheduleId } from '../types/general'
import useScheduling from '../utils/useScheduling'

import { getStartsStops } from './getStartsStops'
import { StartStop } from './types'

const defaultGetNow = () => new Date()
const TWENTY_FOUR_HOURS_IN_MS = 24 * 60 * 60 * 1000

/**
 * @name useMiningScheduling
 * @description hook that:
 * 1. takes user-defined schedules
 * 2. every time schedules change and periodically calculates when mining should be started or stopped
 * 3. every minute checks the start/stop dates and calls start or stop callback for mining with specific node
 * by default it calculates mining starts/stops for next 24h and will recalculate after that period
 *
 * @prop {Schedule[]} schedules - user-defined mining schedules
 * @prop {(miningType: MiningNodeType, schedule: ScheduleId) => void} startMining - callback for mining start
 * @prop {(miningType: MiningNodeType) => void} stopMining - callback for mining stop
 * @prop {() => Date} [getNow] - time provider that has a default value of () => new Date(), introduced mostly for easier mocking in testing
 * @prop {number} [singleSchedulingPeriod] - length of time that hook should calculate schedules for (default is 24h)
 */
const useMiningScheduling = ({
  schedules,
  startMining,
  stopMining,
  getNow = defaultGetNow,
  singleSchedulingPeriod = TWENTY_FOUR_HOURS_IN_MS,
}: {
  schedules: Schedule[]
  startMining: (miningType: MiningNodeType, schedule: ScheduleId) => void
  stopMining: (miningType: MiningNodeType) => void
  getNow?: () => Date
  singleSchedulingPeriod?: number
}) => {
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>()
  const [startStops, setStartStops] = useState<StartStop[]>(() => {
    const from = getNow()
    const to = new Date(from.getTime() + singleSchedulingPeriod)
    return getStartsStops({
      from,
      to,
      schedules,
    })
  })

  useEffect(() => {
    const from = getNow()
    const to = new Date(from.getTime() + singleSchedulingPeriod)
    const ss = getStartsStops({
      from,
      to,
      schedules,
    })
    setStartStops(ss)
  }, [schedules, getNow])

  useEffect(() => {
    timerRef.current = setTimeout(() => {
      const from = getNow()
      const to = new Date(from.getTime() + singleSchedulingPeriod)
      const ss = getStartsStops({
        from,
        to,
        schedules,
      })
      setStartStops(ss)
    }, singleSchedulingPeriod)
  }, [startStops, getNow])

  useEffect(() => {
    return () => {
      // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
      clearTimeout(timerRef.current!)
    }
  }, [])

  const scheduledCallback = useCallback(
    (now: Date) => {
      const starts = startStops.filter(
        ss =>
          startOfMinute(ss.start).getTime() === startOfMinute(now).getTime(),
      )
      const stops = startStops.filter(
        ss => startOfMinute(ss.stop).getTime() === startOfMinute(now).getTime(),
      )

      starts.forEach(start => startMining(start.toMine, start.scheduleId))
      stops.forEach(stop => stopMining(stop.toMine))
    },
    [startStops, startMining, stopMining],
  )

  useScheduling({
    getNow,
    callback: scheduledCallback,
  })
}

export default useMiningScheduling
