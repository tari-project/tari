import React, { useCallback, useMemo, useState } from 'react'

import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { actions as settingsActions } from '../../store/settings'
import {
  selectIsParoleSet,
  selectAreMoneroCredentialsPresent,
} from '../../store/settings/selectors'
import Modal from '../../components/Modal'

import WalletPasswordBox from './WalletPasswordBox'
import AllCredentialsBox from './AllCredentialsBox'
import MoneroCredentialsBox from './MoneroCredentialsBox'
import { WalletParole, MoneroCredentials } from './types'

export const EnsurePasswordsContext = React.createContext<{
  ensureWalletPasswordInStore: (
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    callback: (...a: any[]) => void,
    required: {
      wallet?: boolean
      monero?: boolean
    },
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
  const [required, setRequired] = useState<{
    wallet?: boolean
    monero?: boolean
  }>({ wallet: true, monero: true })

  const ensureWalletPasswordInStore = useCallback(
    (
      callback: () => void,
      required: {
        wallet?: boolean
        monero?: boolean
      },
    ) => {
      if (modalOpen) {
        return
      }

      if (!required.wallet && !required.monero) {
        return
      }

      if (
        (required.wallet && askToUnlockWallet) ||
        (required.monero && askToUnlockMonero)
      ) {
        setRequired(required)
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
    if (monero) {
      // dispatch(settingsActions.setMoneroCredentials(monero))
    }
    setModalOpen(false)
    action()
  }

  const showWalletForm = required.wallet && askToUnlockWallet
  const showMoneroForm = required.monero && askToUnlockMonero

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
        {showWalletForm && !showMoneroForm && (
          <WalletPasswordBox
            onSubmit={(wallet: WalletParole) => saveCredentials({ wallet })}
            pending={false}
          />
        )}
        {showWalletForm && showMoneroForm && (
          <AllCredentialsBox
            onSubmit={(wallet: WalletParole, monero: MoneroCredentials) =>
              saveCredentials({ wallet, monero })
            }
          />
        )}
        {!showWalletForm && showMoneroForm && (
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
