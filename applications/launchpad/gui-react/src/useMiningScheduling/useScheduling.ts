import { useEffect, useRef } from 'react'

const getNextFullMinute = (now: Date): Date => {
  const nextFullMinute = new Date(now)
  nextFullMinute.setUTCMilliseconds(0)
  nextFullMinute.setUTCSeconds(0)
  nextFullMinute.setUTCMinutes(nextFullMinute.getUTCMinutes() + 1)

  return nextFullMinute
}

const useScheduling = ({
  callback,
  getNow = () => new Date(),
}: {
  callback: (d: Date) => void
  getNow?: () => Date
}) => {
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>()
  const intervalRef = useRef<ReturnType<typeof setInterval> | undefined>()

  useEffect(() => {
    const now = getNow()
    callback(now)

    const millisecondsTillNextFullMinute =
      getNextFullMinute(now).getTime() - now.getTime()

    timerRef.current = setTimeout(() => {
      clearTimeout(timerRef.current!)
      callback(getNow())
      intervalRef.current = setInterval(() => {
        clearInterval(intervalRef.current!)
        callback(getNow())
      }, 60 * 1000)
    }, millisecondsTillNextFullMinute)

    return () => {
      clearTimeout(timerRef.current!)
      clearInterval(intervalRef.current!)
    }
  }, [callback])
}

export default useScheduling
