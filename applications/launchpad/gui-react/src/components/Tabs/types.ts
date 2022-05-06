import { ReactNode } from 'react'

export interface TabProp {
  id: string
  content: ReactNode
}

export interface TabsProps {
  tabs: TabProp[]
  selected: string
  onSelect: (id: string) => void
  inverted?: boolean
}
