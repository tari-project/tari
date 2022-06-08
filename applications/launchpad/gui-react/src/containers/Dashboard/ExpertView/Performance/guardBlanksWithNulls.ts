import { SeriesData } from '../../../../components/Charts/TimeSeries/types'
type ChartData = SeriesData['data']

const guardBlanksWithNulls = (data: ChartData, interval = 1000): ChartData => {
  if (!data.length) {
    return data
  }

  const nullsToInsert: { index: number; xValue: number }[] = []

  for (let i = 1; i < data.length; ++i) {
    const a = data[i - 1]
    const b = data[i]

    if (b.x - a.x > interval) {
      nullsToInsert.push({ index: i, xValue: a.x + interval })
      nullsToInsert.push({ index: i, xValue: b.x - interval })
    }
  }

  if (!nullsToInsert.length) {
    return data
  }

  const dataCopy = [...data]
  nullsToInsert.forEach((nullToInsert, i) => {
    dataCopy.splice(nullToInsert.index + i, 0, {
      x: nullToInsert.xValue,
      y: null,
    })
  })

  return dataCopy
}

export default guardBlanksWithNulls
