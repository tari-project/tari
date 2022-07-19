import {
  humanizeTime,
  toT,
  toMicroT,
  formatAmount,
  humanizeEstimatedTime,
} from './Format'

describe('Format', () => {
  it('humanizeTime: should properly convert milliseconds to the readable string', () => {
    // simple case:
    const time1a = new Date(2022, 1, 1, 8, 0, 0, 0)
    const time1b = new Date(2022, 1, 1, 9, 1, 2, 300)
    const time1Expected = '1:01:02'
    const timeDiff1 = Math.abs(Number(time1b) - Number(time1a))
    const time1Result = humanizeTime(timeDiff1)
    expect(time1Result).toBe(time1Expected)

    // 3 days:
    const time2a = new Date(2022, 1, 1, 8, 0, 0, 0)
    const time2b = new Date(2022, 1, 4, 8, 1, 2, 300)
    const time2Expected = '72:01:02'
    const timeDiff2 = Math.abs(Number(time2b) - Number(time2a))
    const time2Result = humanizeTime(timeDiff2)
    expect(time2Result).toBe(time2Expected)

    // 14 days:
    const time3a = new Date(2022, 1, 1, 8, 0, 0, 0)
    const time3b = new Date(2022, 1, 14, 8, 1, 2, 300)
    const time3Expected = '312:01:02'
    const timeDiff3 = Math.abs(Number(time3b) - Number(time3a))
    const time3Result = humanizeTime(timeDiff3)
    expect(time3Result).toBe(time3Expected)

    // milliseconds only:
    const time4a = new Date(2022, 1, 1, 8, 0, 0, 0)
    const time4b = new Date(2022, 1, 1, 8, 0, 0, 300)
    const time4Expected = '0:00:00'
    const timeDiff4 = Math.abs(Number(time4b) - Number(time4a))
    const time4Result = humanizeTime(timeDiff4)
    expect(time4Result).toBe(time4Expected)

    // time is 0:
    const timeDiff5 = 0
    const time5Expected = '0:00:00'
    const time5Result = humanizeTime(timeDiff5)
    expect(time5Result).toBe(time5Expected)
  })

  it('converts Tauri (T) to Micro Tauri (uT)', () => {
    expect(toT(123456789)).toBe(123.456789)
  })

  it('converts Micro Tauri (uT) to Tauri (T)', () => {
    expect(toMicroT(123.456789)).toBe(123456789)
  })

  it('formats the 0 coin amount to 00,000', () => {
    expect(formatAmount(0)).toBe('00,000')
  })

  it('formats the non-zero coin amount to 00,000.00', () => {
    expect(formatAmount(123123123.789)).toBe(
      (123123123.79).toLocaleString([], { maximumFractionDigits: 2 }),
    )
  })

  it('formats the estimated time', () => {
    expect(humanizeEstimatedTime(450)).toBe('7 min')
    expect(humanizeEstimatedTime(65)).toBe('1 min 5 s')
    expect(humanizeEstimatedTime(7896)).toBe('2h 11 min')
    expect(humanizeEstimatedTime(7)).toBe('7 s')
  })
})
