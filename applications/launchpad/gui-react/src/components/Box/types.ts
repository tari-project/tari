import { ReactNode, CSSProperties } from 'react'

type Gradient = {
  start: string
  end: string
  rotation?: number
}

export type BoxProps = {
  children: ReactNode
  border?: boolean
  style?: CSSProperties
  gradient?: Gradient
  testId?: string
}
