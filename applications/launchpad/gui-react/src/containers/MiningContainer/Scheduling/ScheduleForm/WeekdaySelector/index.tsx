import { useTheme } from 'styled-components'

import Text from '../../../../../components/Text'
import t from '../../../../../locales'

import { Weekday } from './styles'

/**
 * @name WeekdaySelector
 * @description controlled form to allow users select weekdays
 *
 * @prop {number[]} days - selected days
 * @prop {(days: number[]) => void} onChange - callback on weekdays change
 */
const WeekdaySelector = ({
  days,
  onChange,
}: {
  days: number[]
  onChange: (days: number[]) => void
}) => {
  const theme = useTheme()

  const toggle = (index: number) => {
    if (days.includes(index)) {
      onChange(days.filter(a => a !== index))
      return
    }

    onChange([...days, index])
  }

  return (
    <div
      style={{
        display: 'flex',
        justifyContent: 'space-between',
      }}
    >
      {Object.values(t.common.weekdayCapitals).map((capital, index) => {
        const color = days.includes(index)
          ? theme.accent
          : theme.inputPlaceholder

        return (
          <Weekday key={`${capital}${index}`} onClick={() => toggle(index)}>
            <Text color={color}>{capital}</Text>
          </Weekday>
        )
      })}
    </div>
  )
}

export default WeekdaySelector
