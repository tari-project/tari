import { useState } from 'react'
import { useTheme } from 'styled-components'
import TimePicker from 'react-time-picker-input'

import { Time } from '../../../../../../types/general'
import Text from '../../../../../../components/Text'
import Button from '../../../../../../components/Button'

import { timeToString, stringToTime } from './utils'
import { DayTimePickerWrapper } from './styles'

const DayTimePicker = ({
  value,
  label,
  onChange,
}: {
  value?: Time
  label?: string
  onChange: (t: Time) => void
}) => {
  const theme = useTheme()

  const [time, setTime] = useState(() => timeToString(value))

  const onTimeChangeHandler = (value: string) => {
    setTime(value)
    onChange(stringToTime(value))
  }

  const isAm = Number(time.substring(0, 2)) < 12
  const toggleAM = () => {
    const hour = Number(time.substring(0, 2))
    const minute = time.substring(3, 5)

    if (hour >= 12) {
      setTime(`${(hour - 12).toString().padStart(2, '0')}:${minute}`)
      return
    }

    setTime(`${(hour + 12).toString().padStart(2, '0')}:${minute}`)
  }

  return (
    <DayTimePickerWrapper>
      {label && (
        <label
          style={{
            marginBottom: theme.spacing(0.5),
            marginTop: theme.spacing(0.5),
          }}
        >
          <Text color={theme.secondary} type='smallMedium'>
            {label}
          </Text>
        </label>
      )}
      <TimePicker
        hour12Format
        allowDelete
        value={time}
        onChange={onTimeChangeHandler}
      />

      <div style={{ display: 'flex', justifyContent: 'space-around' }}>
        <Button variant='text' onClick={toggleAM}>
          <Text color={isAm ? theme.accent : theme.secondary}>AM</Text>
        </Button>
        <Button variant='text' onClick={toggleAM}>
          <Text color={isAm ? theme.secondary : theme.accent}>PM</Text>
        </Button>
      </div>
    </DayTimePickerWrapper>
  )
}

export default DayTimePicker
