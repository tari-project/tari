import { ReactNode } from 'react'

export type TBotMessage = {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  content: string | ReactNode | ((props?: any) => JSX.Element)
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
  onDarkBg?: boolean
  withFadeOutSection?: 'no' | 'dynamic' | 'yes'
  onMessageRender?: (index: number) => void
  onSkip?: () => void
}

export interface TBotMessageHOCProps {
  updateMessageBoxSize?: () => void
}
