import { useEffect, useRef, useCallback, useState } from 'react'

import { startOfMinute } from '../utils/Date'
import { MiningNodeType, Schedule } from '../types/general'

import useScheduling from './useScheduling'
import { getStartsStops } from './getStartsStops'
import { StartStop } from './types'

const defaultGetNow = () => new Date()
const useMiningScheduling = ({
  schedules,
  startMining,
  stopMining,
  getNow = defaultGetNow,
  singleSchedulingPeriod = 24 * 60 * 60 * 1000,
}: {
  schedules: Schedule[]
  startMining: (miningType: MiningNodeType) => void
  stopMining: (miningType: MiningNodeType) => void
  getNow?: () => Date
  singleSchedulingPeriod?: number
}) => {
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>()
  const schedulesRef = useRef(schedules)
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
    // prevent first useEffect when schedules changes from undefined to first prop value
    if (schedulesRef.current === schedules) {
      return
    }

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

      starts.forEach(start => startMining(start.toMine))
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
