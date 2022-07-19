import React, { useCallback, useMemo, useState } from 'react'

import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { actions as credentialsActions } from '../../store/credentials'
import {
  selectIsParoleSet,
  selectAreMoneroCredentialsPresent,
} from '../../store/credentials/selectors'
import Modal from '../../components/Modal'

import WalletPasswordBox from './WalletPasswordBox'
import AllCredentialsBox from './AllCredentialsBox'
import MoneroCredentialsBox from './MoneroCredentialsBox'
import { WalletParole, MoneroCredentials } from './types'
import { selectWalletPasswordConfirmation } from '../../store/temporary/selectors'

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
  const isParoleSet = useAppSelector(selectIsParoleSet)
  const walletPassConfirm = useAppSelector(selectWalletPasswordConfirmation)
  const areMoneroCredentialsPresent = useAppSelector(
    selectAreMoneroCredentialsPresent,
  )

  const [modalOpen, setModalOpen] = useState(false)
  const [action, setAction] = useState<() => void>(() => null)
  const [showWalletForm, setShowWalletForm] = useState(true)
  const [showMoneroForm, setShowMoneroForm] = useState(true)

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
        callback()

        return
      }

      setShowWalletForm(
        Boolean(
          (required.wallet && !isParoleSet) || walletPassConfirm !== 'success',
        ),
      )
      setShowMoneroForm(
        Boolean(required.monero && !areMoneroCredentialsPresent),
      )
      if (
        (required.wallet && !isParoleSet) ||
        walletPassConfirm !== 'success' ||
        (required.monero && !areMoneroCredentialsPresent)
      ) {
        setAction(() => callback)
        setModalOpen(true)
        return
      }

      callback()
    },
    [modalOpen, isParoleSet, areMoneroCredentialsPresent],
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
      dispatch(credentialsActions.setWallet(wallet))
    }
    if (monero) {
      dispatch(credentialsActions.setMoneroCredentials(monero))
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
