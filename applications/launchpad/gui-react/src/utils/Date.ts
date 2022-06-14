export const startOfSecond = (d: Date) => {
  const copy = new Date(d)

  copy.setMilliseconds(0)

  return copy
}

export const startOfYear = (d: Date) => new Date(`${d.getFullYear()}`)
export const endOfYear = (d: Date) => {
  const startOfNextYear = new Date(`${d.getFullYear() + 1}`)
  startOfNextYear.setMilliseconds(-1)

  return startOfNextYear
}

export const startOfMonth = (d: Date) => {
  const copy = new Date(d)

  copy.setDate(1)
  copy.setHours(0)
  copy.setMinutes(0)
  copy.setSeconds(0)
  copy.setMilliseconds(0)

  return copy
}

export const endOfMonth = (d: Date) => {
  const copy = new Date(d)

  if (copy.getMonth() === 11) {
    copy.setMonth(0)
  } else {
    copy.setMonth(copy.getMonth() + 1)
  }

  copy.setDate(1)
  copy.setHours(0)
  copy.setMinutes(0)
  copy.setSeconds(0)
  copy.setMilliseconds(-1)

  return copy
}

export const isCurrentMonth = (d: Date) => {
  const now = new Date()
  return Boolean(
    d.getFullYear() === now.getFullYear() && d.getMonth() === now.getMonth(),
  )
}

export const startOfDay = (d: Date) => {
  const copy = new Date(d)

  copy.setHours(0)
  copy.setMinutes(0)
  copy.setSeconds(0)
  copy.setMilliseconds(0)

  return copy
}

export const startOfUTCDay = (d: Date) => {
  const copy = new Date(d)

  copy.setUTCHours(0)
  copy.setUTCMinutes(0)
  copy.setUTCSeconds(0)
  copy.setUTCMilliseconds(0)

  return copy
}

export const dateInside = (
  date: Date,
  { from, to }: { from: Date; to: Date },
) => date.getTime() >= from.getTime() && date.getTime() <= to.getTime()

export const startOfMinute = (d: Date) => {
  const copy = new Date(d)

  copy.setUTCSeconds(0)
  copy.setUTCMilliseconds(0)

  return copy
}
