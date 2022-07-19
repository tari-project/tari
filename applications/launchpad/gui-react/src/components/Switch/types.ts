import { ReactNode } from 'react'

export interface SwitchProps {
  value: boolean
  leftLabel?: string | ReactNode
  rightLabel?: string | ReactNode
  onClick: (val: boolean) => void
  inverted?: boolean
  disable?: boolean
  testId?: string
}
