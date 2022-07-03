import React, { useCallback, useMemo, useState } from 'react'

import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { actions as settingsActions } from '../../store/settings'
import {
  selectIsParoleSet,
  selectAreMoneroCredentialsPresent,
} from '../../store/settings/selectors'
import Modal from '../../components/Modal'

import WalletPasswordBox, { Overrides } from './PasswordBox'
import AllCredentialsBox from './AllCredentialsBox'
import MoneroCredentialsBox from './MoneroCredentialsBox'
import { WalletParole, MoneroCredentials } from './types'

export const EnsurePasswordsContext = React.createContext<{
  ensureWalletPasswordInStore: (
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    callback: (...a: any[]) => void,
    overrides?: Overrides,
  ) => void
}>({ ensureWalletPasswordInStore: () => null })

const PasswordsPrompt = ({
  children,
  local,
}: {
  children: JSX.Element
  local?: boolean
}) => {
  const dispatch = useAppDispatch()
  const askToUnlockWallet = !useAppSelector(selectIsParoleSet)
  const askToUnlockMonero = !useAppSelector(selectAreMoneroCredentialsPresent)

  const [modalOpen, setModalOpen] = useState(false)
  const [action, setAction] = useState<() => void>(() => null)
  const [overrides, setOverrides] = useState<Overrides | undefined>(undefined)

  const ensureWalletPasswordInStore = useCallback(
    (callback: () => void, actionOverrides?: Overrides) => {
      if (modalOpen) {
        return
      }

      if (askToUnlockWallet || askToUnlockMonero) {
        setOverrides(actionOverrides)
        setAction(() => callback)
        setModalOpen(true)
        return
      }

      // TODO await and error handling?
      callback()
    },
    [askToUnlockWallet, askToUnlockMonero],
  )

  const contextValue = useMemo(
    () => ({
      ensureWalletPasswordInStore,
    }),
    [ensureWalletPasswordInStore],
  )

  const saveCredentials = ({
    wallet,
    monero,
  }: {
    wallet?: WalletParole
    monero?: MoneroCredentials
  }) => {
    if (wallet) {
      dispatch(settingsActions.setParole(wallet))
    }
    setModalOpen(false)
    action()
  }

  return (
    <>
      <EnsurePasswordsContext.Provider value={contextValue}>
        {children}
      </EnsurePasswordsContext.Provider>
      <Modal
        open={modalOpen}
        onClose={() => setModalOpen(false)}
        local={local}
        size='auto'
      >
        {askToUnlockWallet && !askToUnlockMonero && (
          <WalletPasswordBox
            onSubmit={(wallet: WalletParole) => saveCredentials({ wallet })}
            pending={false}
          />
        )}
        {askToUnlockWallet && askToUnlockMonero && (
          <AllCredentialsBox
            onSubmit={(wallet: WalletParole, monero: MoneroCredentials) =>
              saveCredentials({ wallet, monero })
            }
          />
        )}
        {!askToUnlockWallet && askToUnlockMonero && (
          <MoneroCredentialsBox
            onSubmit={(monero: MoneroCredentials) =>
              saveCredentials({ monero })
            }
          />
        )}
      </Modal>
    </>
  )
}

export default PasswordsPrompt
