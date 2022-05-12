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
