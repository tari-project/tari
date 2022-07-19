import { useTheme } from 'styled-components'

import Box from '../Box'
import { BoxProps } from '../Box/types'

const DatePickerWrapper = ({ style, ...props }: BoxProps) => {
  const theme = useTheme()

  return (
    <Box
      style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(7, 1fr)',
        gridTemplateRows: '1fr 2fr',
        gridTemplateAreas: '"month month month month month month month"',
        columnGap: theme.spacing(0.25),
        justifyItems: 'center',
        alignItems: 'center',
        justifyContent: 'center',
        background: theme.nodeBackground,
        border: `1px solid ${theme.selectBorderColor}`,
        ...style,
      }}
      {...props}
    />
  )
}

export default DatePickerWrapper
