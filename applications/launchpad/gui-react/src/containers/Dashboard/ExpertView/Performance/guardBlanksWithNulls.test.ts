import guardBlanksWithNulls from './guardBlanksWithNulls'

describe('guardBlanksWithNulls', () => {
  it('should not add null values if there is no blank', () => {
    // given
    const given = [{ x: new Date().getTime(), y: 123 }]

    // when
    const actual = guardBlanksWithNulls(given)

    // then
    expect(actual).toStrictEqual(given)
  })

  it('should add null at both ends of blank space between data intervals', () => {
    // given
    const firstInterval = [
      { x: new Date('2022-06-08T16:12:31.000Z').getTime(), y: 123 },
      { x: new Date('2022-06-08T16:12:32.000Z').getTime(), y: 124 },
      { x: new Date('2022-06-08T16:12:33.000Z').getTime(), y: 125 },
    ]
    const secondInterval = [
      { x: new Date('2022-06-08T17:12:01.000Z').getTime(), y: 123 },
      { x: new Date('2022-06-08T17:12:02.000Z').getTime(), y: 124 },
      { x: new Date('2022-06-08T17:12:03.000Z').getTime(), y: 125 },
    ]
    const expected = [
      ...firstInterval,
      { x: new Date('2022-06-08T16:12:34.000Z').getTime(), y: null },
      { x: new Date('2022-06-08T17:12:00.000Z').getTime(), y: null },
      ...secondInterval,
    ]

    const given = [...firstInterval, ...secondInterval]

    // when
    const actual = guardBlanksWithNulls(given)

    // then
    expect(actual).toStrictEqual(expected)
  })

  it('should add nulls between all blanks', () => {
    // given
    const firstInterval = [
      { x: new Date('2022-06-08T16:12:31.000Z').getTime(), y: 123 },
      { x: new Date('2022-06-08T16:12:32.000Z').getTime(), y: 124 },
      { x: new Date('2022-06-08T16:12:33.000Z').getTime(), y: 125 },
    ]
    const secondInterval = [
      { x: new Date('2022-06-08T17:12:01.000Z').getTime(), y: 123 },
      { x: new Date('2022-06-08T17:12:02.000Z').getTime(), y: 124 },
      { x: new Date('2022-06-08T17:12:03.000Z').getTime(), y: 125 },
    ]
    const thirdInterval = [
      { x: new Date('2022-06-08T18:12:11.000Z').getTime(), y: 123 },
      { x: new Date('2022-06-08T18:12:12.000Z').getTime(), y: 124 },
      { x: new Date('2022-06-08T18:12:13.000Z').getTime(), y: 125 },
    ]
    const expected = [
      ...firstInterval,
      { x: new Date('2022-06-08T16:12:34.000Z').getTime(), y: null },
      { x: new Date('2022-06-08T17:12:00.000Z').getTime(), y: null },
      ...secondInterval,
      { x: new Date('2022-06-08T17:12:04.000Z').getTime(), y: null },
      { x: new Date('2022-06-08T18:12:10.000Z').getTime(), y: null },
      ...thirdInterval,
    ]

    const given = [...firstInterval, ...secondInterval, ...thirdInterval]

    // when
    const actual = guardBlanksWithNulls(given)

    // then
    expect(actual).toStrictEqual(expected)
  })

  it('should not add nulls if the blank is less than interval', () => {
    // given
    const firstInterval = [
      { x: new Date('2022-06-08T16:12:31.000Z').getTime(), y: 123 },
      { x: new Date('2022-06-08T16:12:32.000Z').getTime(), y: 124 },
      { x: new Date('2022-06-08T16:12:33.000Z').getTime(), y: 125 },
    ]
    const secondInterval = [
      { x: new Date('2022-06-08T17:12:01.000Z').getTime(), y: 123 },
      { x: new Date('2022-06-08T17:12:02.000Z').getTime(), y: 124 },
      { x: new Date('2022-06-08T17:12:03.000Z').getTime(), y: 125 },
    ]
    const intervalBiggerThanBlank =
      secondInterval[0].x - firstInterval[2].x + 1000

    const given = [...firstInterval, ...secondInterval]

    // when
    const actual = guardBlanksWithNulls(given, intervalBiggerThanBlank)

    // then
    expect(actual).toStrictEqual(given)
  })
})
