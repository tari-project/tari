import { ReactNode } from 'react'

export interface TabsProps {
  tabs: {
    id: string
    content: ReactNode
  }[]
  selected: string
  onSelect: (id: string) => void
}
