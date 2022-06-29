import React, { useCallback, useContext, useState } from 'react'

import { useAppSelector, useAppDispatch } from './store/hooks'
import { actions as settingsActions } from './store/settings'
import { selectIsParoleSet } from './store/settings/selectors'
import Modal from './components/Modal'
import PasswordBox from './containers/WalletContainer/PasswordBox'

const EnsureWalletPasswordContext = React.createContext<{
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  ensureWalletPasswordInStore: (callback: (...a: any[]) => void) => void
}>({ ensureWalletPasswordInStore: () => null })

export const WalletPasswordPrompt = ({
  children,
  local,
}: {
  children: JSX.Element
  local?: boolean
}) => {
  const dispatch = useAppDispatch()
  const isParoleSet = useAppSelector(selectIsParoleSet)

  const [modalOpen, setModalOpen] = useState(false)
  const [action, setAction] = useState<() => void>(() => null)

  const ensureWalletPasswordInStore = useCallback(
    (callback: () => void) => {
      if (!isParoleSet) {
        setAction(() => callback)
        setModalOpen(true)
        return
      }

      // TODO await and error handling?
      callback()
    },
    [isParoleSet],
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
          // TODO make async, loader indicator, error indicator (in passwordbox) ??
          onSubmit={parole => {
            dispatch(settingsActions.setParole(parole))
            setModalOpen(false)
            action()
          }}
          style={{ margin: 0 }}
        />
      </Modal>
    </>
  )
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const useWithWalletPassword = (action: (...args: any[]) => void) => {
  const { ensureWalletPasswordInStore } = useContext(
    EnsureWalletPasswordContext,
  )

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (...args: any[]) => ensureWalletPasswordInStore(() => action(...args))
}
