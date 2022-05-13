export const amount = (a: number): string => new Intl.NumberFormat().format(a)

export const hour = ({
  hours,
  minutes,
}: {
  hours: number
  minutes: number
}) => {
  const date = new Date()
  date.setHours(hours)
  date.setMinutes(minutes)

  return date.toLocaleTimeString([], {
    hour: 'numeric',
    minute: '2-digit',
    hour12: true,
  })
}

export const day = (date: Date) =>
  date.toLocaleDateString([], {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  })

/**
 * Convert milliseconds to 0:00:00 {hours:minutes:seconds} format.
 * @param {number} time - milliseconds
 *
 * @example
 * humanizeTime(10000000) // '02:46:40'
 */
export const humanizeTime = (time: number): string => {
  const hours = Math.trunc(time / 3600000).toString()
  const minutes = (Math.trunc(time / 60000) % 60).toString()
  const seconds = (Math.trunc(time / 1000) % 60).toString()

  return `${hours.padStart(1, '0')}:${minutes.padStart(
    2,
    '0',
  )}:${seconds.padStart(2, '0')}`
}
