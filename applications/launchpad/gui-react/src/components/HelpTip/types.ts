import { CSSProperties } from 'react'

export type HelpTipProps = {
  text: string
  cta: string
  onHelp: () => void
  style?: CSSProperties
  header?: boolean
}
