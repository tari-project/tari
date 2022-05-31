import { ReactNode } from 'react'

export interface TBotPromptProps {
  open: boolean
  children?: ReactNode[]
  animate?: boolean
  testid?: string
}
