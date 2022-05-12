import { useTheme } from 'styled-components'

import Text from '../../../../components/Text'
import { day } from '../../../../utils/Format'
import t from '../../../../locales'
import { Schedule } from '../types'

import DayIndicator from './DayIndicator'

const When = ({
  days,
  date,
  disabled,
}: Pick<Schedule, 'days' | 'date'> & { disabled: boolean }) => {
  const theme = useTheme()

  const color = disabled ? theme.placeholderText : undefined

  return (
    <div>
      {days &&
        Object.values(t.common.dayCapitals).map((capital, index) => (
          <DayIndicator
            key={`${capital}${index}`}
            enabled={days.includes(index)}
            disabled={disabled}
          >
            <Text type='smallMedium'>{capital}</Text>
          </DayIndicator>
        ))}
      {date && <Text color={color}>{day(date)}</Text>}
    </div>
  )
}
export default When
