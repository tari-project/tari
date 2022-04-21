export interface SwitchProps {
  value: boolean
  label?: string
  onClick: (val: boolean) => void
  invertedStyle?: boolean
  testId?: string
}
