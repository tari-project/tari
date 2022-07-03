import { useContext } from 'react'

import { Overrides } from './PasswordBox'

import { EnsurePasswordsContext } from '.'

const useWithPasswordPrompt = (
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  action: (...args: any[]) => void,
  overrides?: Overrides,
) => {
  const { ensureWalletPasswordInStore } = useContext(EnsurePasswordsContext)

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (...args: any[]) =>
    ensureWalletPasswordInStore(() => action(...args), overrides)
}

export default useWithPasswordPrompt
