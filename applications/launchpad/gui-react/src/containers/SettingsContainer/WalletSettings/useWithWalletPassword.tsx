import React, { useCallback, useContext, useState } from 'react'

import Modal from '../../../components/Modal'
import PasswordBox from '../../../containers/WalletContainer/PasswordBox'

const EnsureWalletPasswordContext = React.createContext<{
  ensureWalletPasswordInStore: (callback: () => void) => void
}>({ ensureWalletPasswordInStore: () => null })

export const WalletPasswordPrompt = ({
  children,
  local,
}: {
  children: JSX.Element
  local?: boolean
}) => {
  const [modalOpen, setModalOpen] = useState(false)
  const [action, setAction] = useState<() => void>(() => null)
  const passwordAlreadySet = false

  const ensureWalletPasswordInStore = useCallback(
    (callback: () => void) => {
      if (!passwordAlreadySet) {
        setAction(() => callback)
        setModalOpen(true)
        return
      }

      // TODO await and error handling?
      callback()
    },
    [passwordAlreadySet],
  )

  return (
    <>
      <EnsureWalletPasswordContext.Provider
        value={{ ensureWalletPasswordInStore }}
      >
        {children}
      </EnsureWalletPasswordContext.Provider>
      <Modal
        open={modalOpen}
        onClose={() => setModalOpen(false)}
        local={local}
        size='auto'
      >
        <PasswordBox
          pending={false}
          // TODO make async, loader indicator, error indicator (in passwordbox)
          onSubmit={password => {
            // save password in the store
            setModalOpen(false)
            action()
          }}
          style={{ margin: 0 }}
        />
      </Modal>
    </>
  )
}

export const useWithWalletPassword = (action: () => void) => {
  const { ensureWalletPasswordInStore } = useContext(
    EnsureWalletPasswordContext,
  )

  return () => ensureWalletPasswordInStore(action)
}
