import { CSSProperties } from 'react'

export type DatePickerProps = {
  value?: Date
  open: boolean
  onChange: (d: Date) => void
  style?: CSSProperties
}
