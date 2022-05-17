import DatePickerComponent from './DatePicker'

import { DatePickerProps } from './types'

const DatePicker = ({ open, ...props }: DatePickerProps) => {
  if (!open) {
    return null
  }

  return <DatePickerComponent {...props} />
}

export default DatePicker
