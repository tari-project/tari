export const createPeriodicalGetNow = (
  start: Date,
  period: number,
): {
  getNow: () => Date
  returnedDates: Date[]
} => {
  const from = new Date(start)
  let counter = 0
  const returnedDates = [] as Date[]

  const getNow = jest.fn(() => {
    const newNow = new Date(from.getTime() + counter++ * period)

    returnedDates.push(newNow)

    return newNow
  })

  return { getNow, returnedDates }
}
