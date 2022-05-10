export interface WalletPasswordFormProps {
  submitBtnText?: string
  onSubmit: (data: WalletPasswordInputs) => Promise<void>
}

export interface WalletPasswordInputs {
  password: string
}
