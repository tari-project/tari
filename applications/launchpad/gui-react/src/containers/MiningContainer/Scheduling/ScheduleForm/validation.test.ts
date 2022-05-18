import { Interval, Schedule } from '../../../../types/general'
import t from '../../../../locales'

import { validate } from './validation'
import { timeToString } from './utils'

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
      it(`from: ${timeToString(interval.from)} to: ${timeToString(
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
})
