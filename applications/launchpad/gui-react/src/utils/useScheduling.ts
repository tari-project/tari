import { useEffect, useRef } from 'react'

const getNextFullMinute = (now: Date): Date => {
  const nextFullMinute = new Date(now)
  nextFullMinute.setUTCMilliseconds(0)
  nextFullMinute.setUTCSeconds(0)
  nextFullMinute.setUTCMinutes(nextFullMinute.getUTCMinutes() + 1)

  return nextFullMinute
}

const defaultGetNow = () => new Date()

/**
 * @name useScheduling
 * @description long living hook that runs specified callback every full minute
 *
 * @prop {(d: Date) => void} callback - callback to be run on every minute (with that minute passed)
 * @prop {() => Date} [getNow] - time provider with default value returning new Date() (mostly for testing)
 */
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
    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    clearTimeout(timerRef.current!)
    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    clearInterval(intervalRef.current!)

    const now = getNow()
    callback(now)

    const millisecondsTillNextFullMinute =
      getNextFullMinute(now).getTime() - now.getTime()

    timerRef.current = setTimeout(() => {
      callback(getNow())
      intervalRef.current = setInterval(() => {
        callback(getNow())
      }, 60 * 1000)
    }, millisecondsTillNextFullMinute)
  }, [callback])

  useEffect(() => {
    return () => {
      // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
      clearTimeout(timerRef.current!)
      // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
      clearInterval(intervalRef.current!)
    }
  }, [])
}

export default useScheduling
