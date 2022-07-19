import { ReactNode } from 'react'

export type CalloutType = 'warning'

export interface CalloutProps {
  type?: CalloutType
  icon?: string | ReactNode
  inverted?: boolean
  children: string | ReactNode
}
