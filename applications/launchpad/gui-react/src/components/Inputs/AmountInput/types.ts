import { ReactNode } from 'react'

export interface AmountInputProps {
  value?: number
  onChange: (val: number) => void
  disabled?: boolean
  icon?: ReactNode
  error?: string
  withError?: boolean
  fee?: number
  withFee?: boolean
  feeHelp?: boolean
  testId?: string
  maxDecimals?: number
  currency?: string
  autoFocus?: boolean
}
