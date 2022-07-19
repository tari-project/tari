import { useTheme } from 'styled-components'

import { Interval } from '../../../../../types/general'

import DayTimePicker from './DayTimePicker'

const IntervalPicker = ({
  value,
  onChange,
}: {
  value: Interval
  onChange: (v: Interval) => void
}) => {
  const theme = useTheme()

  return (
    <div
      style={{
        backgroundColor: theme.selectOptionHover,
        borderRadius: theme.borderRadius(),
        display: 'flex',
        justifyContent: 'space-around',
        padding: theme.spacing(),
      }}
    >
      <DayTimePicker
        value={value?.from}
        label='Start'
        onChange={from =>
          onChange({
            ...value,
            from,
          })
        }
      />
      <div
        style={{
          width: '1px',
          minWidth: '1px',
          background: theme.borderColor,
          height: '100%',
        }}
      />
      <DayTimePicker
        value={value?.to}
        label='Stop'
        onChange={to =>
          onChange({
            ...value,
            to,
          })
        }
      />
    </div>
  )
}

export default IntervalPicker
