import { Schedule } from '../types/general'
import { startOfUTCDay } from '../utils/Date'

import { StartStop } from './types'
import { getStartsStops } from './getStartsStops'

describe('getStartsStops', () => {
  it('should generate single start stop from single tari mining date Schedule', () => {
    // given
    const today = new Date('2022-05-21T00:00:00.000Z')
    const from = new Date('2022-05-21T00:00:00.000Z')
    const to = new Date('2022-05-21T23:00:00.000Z')

    const singleSchedule: Schedule[] = [
      {
        id: 'scheduleId',
        enabled: true,
        date: today,
        interval: {
          from: {
            hours: 17,
            minutes: 0,
          },
          to: {
            hours: 18,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
    ]
    const expected: StartStop[] = [
      {
        start: new Date('2022-05-21T17:00:00.000Z'),
        stop: new Date('2022-05-21T18:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'scheduleId',
      },
    ]

    // when
    const actual = getStartsStops({
      from,
      to,
      schedules: singleSchedule,
    })

    // then
    expect(actual).toEqual(expected)
  })

  it('should generate startStop if only start is inside from-to', () => {
    // given
    const from = new Date('2022-05-21T09:00:00.000Z')
    const to = new Date('2022-05-21T13:00:00.000Z')

    const singleSchedule: Schedule[] = [
      {
        id: 'scheduleId',
        enabled: true,
        date: startOfUTCDay(from),
        interval: {
          from: {
            hours: 12,
            minutes: 0,
          },
          to: {
            hours: 18,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
    ]
    const expected: StartStop[] = [
      {
        start: new Date('2022-05-21T12:00:00.000Z'),
        stop: new Date('2022-05-21T18:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'scheduleId',
      },
    ]

    // when
    const actual = getStartsStops({
      from,
      to,
      schedules: singleSchedule,
    })

    // then
    expect(actual).toEqual(expected)
  })

  it('should generate startStop if only stop is inside from-to', () => {
    // given
    const from = new Date('2022-05-21T09:00:00.000Z')
    const to = new Date('2022-05-21T13:00:00.000Z')

    const singleSchedule: Schedule[] = [
      {
        id: 'scheduleId',
        enabled: true,
        date: startOfUTCDay(from),
        interval: {
          from: {
            hours: 7,
            minutes: 0,
          },
          to: {
            hours: 12,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
    ]
    const expected: StartStop[] = [
      {
        start: new Date('2022-05-21T09:00:00.000Z'),
        stop: new Date('2022-05-21T12:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'scheduleId',
      },
    ]

    // when
    const actual = getStartsStops({
      from,
      to,
      schedules: singleSchedule,
    })

    // then
    expect(actual).toEqual(expected)
  })

  it('should ignore schedules from the past', () => {
    // given
    const from = new Date('2022-05-21T09:00:00.000Z')
    const to = new Date('2022-05-21T13:00:00.000Z')

    const singleSchedule: Schedule[] = [
      {
        id: 'scheduleId',
        enabled: true,
        date: startOfUTCDay(from),
        interval: {
          from: {
            hours: 7,
            minutes: 0,
          },
          to: {
            hours: 8,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
    ]
    const expected: StartStop[] = []

    // when
    const actual = getStartsStops({
      from,
      to,
      schedules: singleSchedule,
    })

    // then
    expect(actual).toEqual(expected)
  })

  it('should generate two start stop from date Schedule with two mining types', () => {
    // given
    const today = new Date('2022-05-21T00:00:00.000Z')
    const from = new Date(today)
    const to = new Date(today.setUTCHours(23))

    const singleSchedule: Schedule[] = [
      {
        id: 'scheduleId',
        enabled: true,
        date: today,
        interval: {
          from: {
            hours: 17,
            minutes: 0,
          },
          to: {
            hours: 18,
            minutes: 0,
          },
        },
        type: ['tari', 'merged'],
      },
    ]
    const expected: StartStop[] = [
      {
        start: new Date('2022-05-21T17:00:00.000Z'),
        stop: new Date('2022-05-21T18:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'scheduleId',
      },
      {
        start: new Date('2022-05-21T17:00:00.000Z'),
        stop: new Date('2022-05-21T18:00:00.000Z'),
        toMine: 'merged',
        scheduleId: 'scheduleId',
      },
    ]

    // when
    const actual = getStartsStops({
      from,
      to,
      schedules: singleSchedule,
    })

    // then
    expect(actual).toEqual(expected)
  })

  it('should ignore disabled schedules', () => {
    // given
    const today = new Date('2022-05-21T00:00:00.000Z')
    const from = new Date(today)
    const to = new Date(today.setUTCHours(23))

    const singleSchedule: Schedule[] = [
      {
        id: 'scheduleId',
        enabled: true,
        date: today,
        interval: {
          from: {
            hours: 17,
            minutes: 0,
          },
          to: {
            hours: 18,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
      {
        id: 'otherScheduleId',
        enabled: false,
        date: today,
        interval: {
          from: {
            hours: 16,
            minutes: 0,
          },
          to: {
            hours: 16,
            minutes: 5,
          },
        },
        type: ['tari'],
      },
    ]
    const expected: StartStop[] = [
      {
        start: new Date('2022-05-21T17:00:00.000Z'),
        stop: new Date('2022-05-21T18:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'scheduleId',
      },
    ]

    // when
    const actual = getStartsStops({
      from,
      to,
      schedules: singleSchedule,
    })

    // then
    expect(actual).toEqual(expected)
  })

  it('should ignore enabled schedules outside the from-to range', () => {
    // given
    const today = new Date('2022-05-21T00:00:00.000Z')
    const tomorrow = new Date('2022-05-22T00:00:00.000Z')
    const from = new Date(today)
    const to = new Date(today.setUTCHours(23))

    const singleSchedule: Schedule[] = [
      {
        id: 'scheduleId',
        enabled: true,
        date: today,
        interval: {
          from: {
            hours: 17,
            minutes: 0,
          },
          to: {
            hours: 18,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
      {
        id: 'otherScheduleId',
        enabled: true,
        date: tomorrow,
        interval: {
          from: {
            hours: 16,
            minutes: 0,
          },
          to: {
            hours: 16,
            minutes: 5,
          },
        },
        type: ['tari'],
      },
    ]
    const expected: StartStop[] = [
      {
        start: new Date('2022-05-21T17:00:00.000Z'),
        stop: new Date('2022-05-21T18:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'scheduleId',
      },
    ]

    // when
    const actual = getStartsStops({
      from,
      to,
      schedules: singleSchedule,
    })

    // then
    expect(actual).toEqual(expected)
  })

  it('should merge overlapping date schedules', () => {
    // given
    const today = new Date('2022-05-21T00:00:00.000Z')
    const from = new Date(today)
    const to = new Date(today.setUTCHours(23))

    const singleSchedule: Schedule[] = [
      {
        id: 'scheduleId',
        enabled: true,
        date: today,
        interval: {
          from: {
            hours: 17,
            minutes: 0,
          },
          to: {
            hours: 18,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
      {
        id: 'otherScheduleId',
        enabled: true,
        date: today,
        interval: {
          from: {
            hours: 16,
            minutes: 0,
          },
          to: {
            hours: 17,
            minutes: 30,
          },
        },
        type: ['tari'],
      },
    ]
    const expected: StartStop[] = [
      {
        start: new Date('2022-05-21T16:00:00.000Z'),
        stop: new Date('2022-05-21T18:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'otherScheduleId',
      },
    ]

    // when
    const actual = getStartsStops({
      from,
      to,
      schedules: singleSchedule,
    })

    // then
    expect(actual).toEqual(expected)
  })

  it('should merge overlapping date schedules even if later schedule ends after `to`', () => {
    // given
    const today = new Date('2022-05-21T00:00:00.000Z')
    const from = new Date(today.setUTCHours(2))
    const to = new Date(today.setUTCHours(18))

    const singleSchedule: Schedule[] = [
      {
        id: 'scheduleId',
        enabled: true,
        date: today,
        interval: {
          from: {
            hours: 17,
            minutes: 0,
          },
          to: {
            hours: 19,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
      {
        id: 'otherScheduleId',
        enabled: true,
        date: today,
        interval: {
          from: {
            hours: 16,
            minutes: 0,
          },
          to: {
            hours: 17,
            minutes: 30,
          },
        },
        type: ['tari'],
      },
    ]
    const expected: StartStop[] = [
      {
        start: new Date('2022-05-21T16:00:00.000Z'),
        stop: new Date('2022-05-21T19:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'otherScheduleId',
      },
    ]

    // when
    const actual = getStartsStops({
      from,
      to,
      schedules: singleSchedule,
    })

    // then
    expect(actual).toEqual(expected)
  })

  it('should return startstops ordered ascending by start time', () => {
    // given
    const today = new Date('2022-05-21T00:00:00.000Z')
    const from = new Date(today.setUTCHours(2))
    const to = new Date(today.setUTCHours(23))

    const singleSchedule: Schedule[] = [
      {
        id: 'scheduleId',
        enabled: true,
        date: today,
        interval: {
          from: {
            hours: 17,
            minutes: 0,
          },
          to: {
            hours: 18,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
      {
        id: 'anotherScheduleId',
        enabled: true,
        date: today,
        interval: {
          from: {
            hours: 16,
            minutes: 50,
          },
          to: {
            hours: 20,
            minutes: 0,
          },
        },
        type: ['merged'],
      },
      {
        id: 'otherScheduleId',
        enabled: true,
        date: today,
        interval: {
          from: {
            hours: 13,
            minutes: 0,
          },
          to: {
            hours: 14,
            minutes: 40,
          },
        },
        type: ['tari'],
      },
    ]
    const expected: StartStop[] = [
      {
        start: new Date('2022-05-21T13:00:00.000Z'),
        stop: new Date('2022-05-21T14:40:00.000Z'),
        toMine: 'tari',
        scheduleId: 'otherScheduleId',
      },
      {
        start: new Date('2022-05-21T16:50:00.000Z'),
        stop: new Date('2022-05-21T20:00:00.000Z'),
        toMine: 'merged',
        scheduleId: 'anotherScheduleId',
      },
      {
        start: new Date('2022-05-21T17:00:00.000Z'),
        stop: new Date('2022-05-21T18:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'scheduleId',
      },
    ]

    // when
    const actual = getStartsStops({
      from,
      to,
      schedules: singleSchedule,
    })

    // then
    expect(actual).toEqual(expected)
  })

  it('should consider recurring schedules', () => {
    // given
    const today = new Date('2022-05-21T00:00:00.000Z') // Saturday
    const from = new Date(today.setUTCHours(2))
    const to = new Date(today.setUTCHours(23))

    const schedules: Schedule[] = [
      {
        id: 'todayEarly',
        enabled: true,
        date: today,
        interval: {
          from: {
            hours: 8,
            minutes: 0,
          },
          to: {
            hours: 11,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
      {
        id: 'onSaturdaysLate',
        enabled: true,
        days: [1, 3, 6],
        interval: {
          from: {
            hours: 19,
            minutes: 0,
          },
          to: {
            hours: 20,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
      {
        id: 'notOnSaturdays',
        enabled: true,
        days: [2, 3],
        interval: {
          from: {
            hours: 17,
            minutes: 0,
          },
          to: {
            hours: 18,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
    ]
    const expected = [
      {
        start: new Date('2022-05-21T08:00:00.000Z'),
        stop: new Date('2022-05-21T11:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'todayEarly',
      },
      {
        start: new Date('2022-05-21T19:00:00.000Z'),
        stop: new Date('2022-05-21T20:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'onSaturdaysLate',
      },
    ]

    // when
    const actual = getStartsStops({
      from,
      to,
      schedules,
    })

    // then
    expect(actual).toEqual(expected)
  })

  it('should consider recurring schedules when from/to spans multiple days', () => {
    // given
    const today = new Date('2022-05-21T00:00:00.000Z') // Saturday
    const nextWed = new Date('2022-05-25T00:00:00.000Z')
    const from = new Date(today.setUTCHours(3))
    const to = new Date(nextWed.setUTCHours(2))

    const schedules: Schedule[] = [
      {
        id: 'early',
        enabled: true,
        days: [0, 1, 6],
        interval: {
          from: {
            hours: 8,
            minutes: 0,
          },
          to: {
            hours: 11,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
      {
        id: 'onSaturdaysLate',
        enabled: true,
        days: [1, 6],
        interval: {
          from: {
            hours: 19,
            minutes: 0,
          },
          to: {
            hours: 20,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
    ]
    const expected = [
      {
        start: new Date('2022-05-21T08:00:00.000Z'),
        stop: new Date('2022-05-21T11:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'early',
      },
      {
        start: new Date('2022-05-21T19:00:00.000Z'),
        stop: new Date('2022-05-21T20:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'onSaturdaysLate',
      },
      {
        start: new Date('2022-05-22T08:00:00.000Z'),
        stop: new Date('2022-05-22T11:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'early',
      },
      {
        start: new Date('2022-05-23T08:00:00.000Z'),
        stop: new Date('2022-05-23T11:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'early',
      },
      {
        start: new Date('2022-05-23T19:00:00.000Z'),
        stop: new Date('2022-05-23T20:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'onSaturdaysLate',
      },
    ]

    // when
    const actual = getStartsStops({
      from,
      to,
      schedules,
    })

    // then
    expect(actual).toEqual(expected)
  })

  it('should merge recurring schedules and specific date schedules', () => {
    // given
    const today = new Date('2022-05-21T00:00:00.000Z') // Saturday
    const tomorrow = new Date('2022-05-22T00:00:00.000Z')
    const from = new Date(today.setUTCHours(3))
    const to = new Date(tomorrow.setUTCHours(11))

    const schedules: Schedule[] = [
      {
        id: 'first',
        enabled: true,
        days: [0, 6],
        interval: {
          from: {
            hours: 8,
            minutes: 0,
          },
          to: {
            hours: 11,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
      {
        id: 'second',
        enabled: true,
        days: [6],
        interval: {
          from: {
            hours: 10,
            minutes: 0,
          },
          to: {
            hours: 12,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
      {
        id: 'specificDate',
        enabled: true,
        date: new Date(tomorrow.setUTCHours(7)),
        interval: {
          from: {
            hours: 11,
            minutes: 0,
          },
          to: {
            hours: 13,
            minutes: 0,
          },
        },
        type: ['tari'],
      },
    ]
    const expected = [
      {
        start: new Date('2022-05-21T08:00:00.000Z'),
        stop: new Date('2022-05-21T12:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'first',
      },
      {
        start: new Date('2022-05-22T08:00:00.000Z'),
        stop: new Date('2022-05-22T13:00:00.000Z'),
        toMine: 'tari',
        scheduleId: 'first',
      },
    ]

    // when
    const actual = getStartsStops({
      from,
      to,
      schedules,
    })

    // then
    expect(actual).toEqual(expected)
  })
})
