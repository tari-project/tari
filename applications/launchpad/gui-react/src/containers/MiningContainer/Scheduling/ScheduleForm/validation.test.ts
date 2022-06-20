import { Interval, Schedule } from '../../../../types/general'
import t from '../../../../locales'

import { validate } from './validation'
import { utcTimeToString } from './utils'

describe('validate', () => {
  describe('interval', () => {
    const testCases: Array<[Interval, string | undefined]> = [
      [
        {
          from: { hours: 7, minutes: 23 },
          to: { hours: 8, minutes: 0 },
        },
        undefined,
      ],
      [
        {
          from: { hours: 7, minutes: 23 },
          to: { hours: 6, minutes: 0 },
        },
        t.mining.scheduling.error_miningEndsBeforeItStarts,
      ],
      [
        {
          from: { hours: 7, minutes: 23 },
          to: { hours: 7, minutes: 12 },
        },
        t.mining.scheduling.error_miningEndsBeforeItStarts,
      ],
      [
        {
          from: { hours: 7, minutes: 23 },
          to: { hours: 7, minutes: 23 },
        },
        t.mining.scheduling.error_miningEndsWhenItStarts,
      ],
    ]

    testCases.forEach(([interval, expected]) =>
      it(`from: ${utcTimeToString(interval.from)} to: ${utcTimeToString(
        interval.to,
      )} expected: ${expected}`, () => {
        const schedule: Schedule = {
          id: Date.now().toString(),
          enabled: true,
          days: [0],
          interval,
          type: ['tari'],
        }
        const error = validate(schedule)

        if (expected) {
          expect(error).toBe(expected)
        } else {
          expect(error).toBeUndefined()
        }
      }),
    )
  })

  describe('date', () => {
    it('should validate undefined date as long as any day is selected', () => {
      const schedule: Schedule = {
        id: Date.now().toString(),
        enabled: true,
        days: [0],
        interval: {
          from: { hours: 7, minutes: 23 },
          to: { hours: 8, minutes: 12 },
        },
        type: ['tari'],
      }

      const error = validate(schedule)

      expect(error).toBeUndefined()
    })

    /* eslint-disable-next-line quotes */
    it(`should validate today's date if interval ends later than now`, () => {
      const now = new Date('2022-05-12T12:00:00.000Z')
      jest.useFakeTimers().setSystemTime(now)

      const schedule: Schedule = {
        id: Date.now().toString(),
        enabled: true,
        date: now,
        interval: {
          from: { hours: 11, minutes: 0 },
          to: { hours: 12, minutes: 30 },
        },
        type: ['tari'],
      }

      const error = validate(schedule)

      expect(error).toBeUndefined()
    })

    /* eslint-disable-next-line quotes */
    it(`should validate today's date if interval is still in the future`, () => {
      const now = new Date('2022-05-12T12:00:00.000Z')
      jest.useFakeTimers().setSystemTime(now)

      const schedule: Schedule = {
        id: Date.now().toString(),
        enabled: true,
        date: now,
        interval: {
          from: { hours: 17, minutes: 23 },
          to: { hours: 19, minutes: 12 },
        },
        type: ['tari'],
      }

      const error = validate(schedule)

      expect(error).toBeUndefined()
    })

    it('should return error for a date in the past', () => {
      const dateInThePast = new Date()
      dateInThePast.setDate(-1)

      const schedule: Schedule = {
        id: Date.now().toString(),
        enabled: true,
        date: dateInThePast,
        interval: {
          from: { hours: 7, minutes: 23 },
          to: { hours: 8, minutes: 12 },
        },
        type: ['tari'],
      }

      const error = validate(schedule)

      expect(error).toBe(t.mining.scheduling.error_miningInThePast)
    })

    /* eslint-disable-next-line quotes */
    it(`should return error for today's date but interval is in the past`, () => {
      const now = new Date('2022-05-12T12:00:00.000Z')
      jest.useFakeTimers().setSystemTime(now)

      const schedule: Schedule = {
        id: Date.now().toString(),
        enabled: true,
        date: now,
        interval: {
          from: { hours: 7, minutes: 23 },
          to: { hours: 8, minutes: 12 },
        },
        type: ['tari'],
      }

      const error = validate(schedule)

      expect(error).toBe(t.mining.scheduling.error_miningInThePast)
    })
  })
})
