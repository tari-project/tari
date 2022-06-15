import { ReactNode } from 'react'

export type TBotMessage = {
  content: string | ReactNode | (() => JSX.Element)
  wait?: number
  barFill?: number
  noSkip?: boolean
}

export interface TBotPromptProps {
  open: boolean
  floating?: boolean
  testid?: string
  messages?: (string | ReactNode | TBotMessage)[]
  currentIndex?: number
  closeIcon?: boolean
  mode?: 'onboarding' | 'help'
}
