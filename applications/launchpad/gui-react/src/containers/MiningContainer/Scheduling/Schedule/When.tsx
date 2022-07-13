import { useTheme } from 'styled-components'

import Text from '../../../../components/Text'
import { day } from '../../../../utils/Format'
import t from '../../../../locales'
import { Schedule } from '../../../../types/general'

import DayIndicator from './DayIndicator'

/**
 * @name When
 * @description renders days of the week with enabled state or date
 *
 * @prop {number[]} [days] - list of days of the week that should be shown as active (0-Sunday, 1-Monday...)
 * @prop {Date} [date] - specific date to be displayed
 * @prop {boolean} disabled - indicates whether to render in disabled UI state
 *
 * @example
 * <When days={[0, 3]} /> - renders days of the week with Sunday and Wednesday in enabled state
 * <When date={new Date('2022-05-13')} /> - renders 13th of May in localized string
 */
const When = ({
  days,
  date,
  disabled,
}: Pick<Schedule, 'days' | 'date'> & { disabled: boolean }) => {
  const theme = useTheme()

  const color = disabled ? theme.inputPlaceholder : theme.primary

  return (
    <div>
      {days &&
        Object.values(t.common.weekdayCapitals).map((capital, index) => (
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
