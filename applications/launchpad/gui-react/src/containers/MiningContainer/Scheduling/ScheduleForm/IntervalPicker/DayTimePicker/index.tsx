import { useState } from 'react'
import { useTheme } from 'styled-components'
import TimePicker from 'react-time-picker-input'

import { Time } from '../../../../../../types/general'
import Text from '../../../../../../components/Text'
import Button from '../../../../../../components/Button'
import { utcTimeToString, stringToUTCTime } from '../../utils'

import { DayTimePickerWrapper } from './styles'

/**
 * @name DayTimePicker
 * @description form input for 12 hour clock with am/pm toggle
 *
 * @prop {Time} [value] - initial value for the picker
 * @prop {string} [label] - label for the value
 * @prop {(t: Time) => void} onChange - callback called when value changes
 */
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

  const [time, setTime] = useState(() => utcTimeToString(value))

  const onTimeChangeHandler = (value: string) => {
    setTime(value)
    onChange(stringToUTCTime(value))
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
          <Text color={theme.nodeWarningText} type='smallMedium'>
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
