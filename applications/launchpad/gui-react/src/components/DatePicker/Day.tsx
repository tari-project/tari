import { useTheme } from 'styled-components'

import Button from '../Button'
import { ButtonProps } from '../Button/types'

const Day = ({ selected, ...props }: ButtonProps & { selected: boolean }) => {
  const theme = useTheme()

  return (
    <Button
      style={{
        padding: 0,
        margin: 0,
        width: '38px',
        height: '38px',
        justifyContent: 'center',
        backgroundColor: selected ? theme.onTextLight : '',
        border: selected ? `2px solid ${theme.on}` : '',
        color: selected ? theme.on : theme.calendarNumber,
        borderRadius: '50%',
      }}
      {...props}
    />
  )
}

export default Day
