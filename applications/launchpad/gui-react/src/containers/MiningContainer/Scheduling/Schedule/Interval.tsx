import { useTheme } from 'styled-components'

import { utcHour } from '../../../../utils/Format'
import Text from '../../../../components/Text'
import { Interval as IntervalType } from '../../../../types/general'

/**
 * @name Interval
 * @description Renders a time window during a single day by from/to hours
 *
 * @prop {Time} from - starting utcHour of interval
 * @prop {Time} to - ending utcHour of interval
 * @prop {boolean} disabled - indicates whether to render in disabled UI state
 *
 * @typedef Time
 * @prop {number} hours - 24h clock indication of the utcHour
 * @prop {number} minutes - minutes of the time
 */
const Interval = ({
  from,
  to,
  disabled,
}: IntervalType & { disabled: boolean }) => {
  const theme = useTheme()

  const color = disabled ? theme.inputPlaceholder : theme.primary
  return (
    <Text type='subheader' color={color}>
      {utcHour(from)} - {utcHour(to)}
    </Text>
  )
}

export default Interval
