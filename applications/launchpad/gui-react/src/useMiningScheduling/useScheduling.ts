import { useEffect, useRef } from 'react'

const getNextFullMinute = (now: Date): Date => {
  const nextFullMinute = new Date(now)
  nextFullMinute.setUTCMilliseconds(0)
  nextFullMinute.setUTCSeconds(0)
  nextFullMinute.setUTCMinutes(nextFullMinute.getUTCMinutes() + 1)

  return nextFullMinute
}

const defaultGetNow = () => new Date()
const useScheduling = ({
  callback,
  getNow = defaultGetNow,
}: {
  callback: (d: Date) => void
  getNow?: () => Date
}) => {
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>()
  const intervalRef = useRef<ReturnType<typeof setInterval> | undefined>()

  useEffect(() => {
    callback(getNow())
  }, [callback])

  useEffect(() => {
    const now = getNow()

    const millisecondsTillNextFullMinute =
      getNextFullMinute(now).getTime() - now.getTime()

    timerRef.current = setTimeout(() => {
      clearInterval(intervalRef.current!)
      callback(getNow())
      intervalRef.current = setInterval(() => {
        callback(getNow())
      }, 60 * 1000)
    }, millisecondsTillNextFullMinute)
  }, [callback])

  useEffect(() => {
    return () => {
      clearTimeout(timerRef.current!)
      clearInterval(intervalRef.current!)
    }
  }, [])
}

export default useScheduling
