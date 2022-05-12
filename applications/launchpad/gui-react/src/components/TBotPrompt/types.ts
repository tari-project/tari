import { ReactNode } from 'react'

export interface TBotPromptProps {
  open?: boolean
  onClose?: () => void
  children?: ReactNode
  testid?: string
}
