export interface SendModalProps {
  open: boolean
  onClose: () => void
  available: number
}

export interface SendForm {
  amount: number
  address: string
  message?: string
}
