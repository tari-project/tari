import { useCallback, useContext } from 'react'

import { EnsurePasswordsContext } from '.'

export type WithRequiredCredentials = {
  wallet?: boolean
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  monero?: boolean | ((...args: any[]) => boolean)
}

const useWithPasswordPrompt = (
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  action: (...args: any[]) => void,
  required: WithRequiredCredentials,
) => {
  const { ensureWalletPasswordInStore } = useContext(EnsurePasswordsContext)

  return useCallback(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (...args: any[]) => {
      ensureWalletPasswordInStore(() => action(...args), {
        wallet: required.wallet,
        monero:
          required.monero &&
          (typeof required.monero === 'function'
            ? required.monero(...args)
            : required.monero),
      })
    },
    [required, ensureWalletPasswordInStore],
  )
}

export default useWithPasswordPrompt
