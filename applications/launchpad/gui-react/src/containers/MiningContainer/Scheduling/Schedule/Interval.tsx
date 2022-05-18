import { useTheme } from 'styled-components'

import { hour } from '../../../../utils/Format'
import Text from '../../../../components/Text'
import { Interval as IntervalType } from '../../../../types/general'

/**
 * @name Interval
 * @description Renders a time window during a single day by from/to hours
 *
 * @prop {Time} from - starting hour of interval
 * @prop {Time} to - ending hour of interval
 * @prop {boolean} disabled - indicates whether to render in disabled UI state
 *
 * @typedef Time
 * @prop {number} hours - 24h clock indication of the hour
 * @prop {number} minutes - minutes of the time
 */
const Interval = ({
  from,
  to,
  disabled,
}: IntervalType & { disabled: boolean }) => {
  const theme = useTheme()

  const color = disabled ? theme.placeholderText : undefined
  return (
    <Text type='subheader' color={color}>
      {hour(from)} - {hour(to)}
    </Text>
  )
}

export default Interval
