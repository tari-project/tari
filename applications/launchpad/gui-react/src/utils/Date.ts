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

export const startOfMonth = (d: Date) => {
  const copy = new Date(d)

  copy.setDate(1)
  copy.setHours(0)
  copy.setMinutes(0)
  copy.setSeconds(0)
  copy.setMilliseconds(0)

  return copy
}

export const isCurrentMonth = (d: Date) => {
  const now = new Date()
  return Boolean(
    d.getFullYear() === now.getFullYear() && d.getMonth() === now.getMonth(),
  )
}

export const clearTime = (d: Date) => {
  const copy = new Date(d)

  copy.setHours(0)
  copy.setMinutes(0)
  copy.setSeconds(0)
  copy.setMilliseconds(0)

  return copy
}
