import { ReactNode } from 'react'

export interface TBotPromptProps {
  open: boolean
  floating?: boolean
  testid?: string
  messages?: (
    | string
    | ReactNode
    | {
        content: string | ReactNode
        wait?: number
      }
  )[]
  currentIndex?: number
  closeIcon?: boolean
}
