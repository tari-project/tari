import DatePickerComponent from './DatePicker'

import { DatePickerProps } from './types'

/**
 * @name DatePicker
 * @description DatePicker container that renders DatePicker according to `open` state and passes props
 *
 * @prop {boolean} open - whether calendar should be open
 * @prop {Date} [value] - selected value
 * @prop {(d: Date) => void} onChange - callback called when user selects a date
 * style {CSSProperties} [style] - optional styles to main container of the date picker
 */
const DatePicker = ({ open, ...props }: DatePickerProps) => {
  if (!open) {
    return null
  }

  return <DatePickerComponent {...props} />
}

export default DatePicker
