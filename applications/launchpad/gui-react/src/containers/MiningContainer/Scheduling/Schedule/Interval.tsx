import { useTheme } from 'styled-components'

import { hour } from '../../../../utils/Format'
import Text from '../../../../components/Text'
import { Interval as IntervalType } from '../../../../types/general'

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
