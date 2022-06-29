import React, { useCallback, useContext, useState } from 'react'

import { useAppSelector, useAppDispatch } from './store/hooks'
import { actions as settingsActions } from './store/settings'
import { selectIsParoleSet } from './store/settings/selectors'
import Modal from './components/Modal'
import PasswordBox, {
  Overrides,
} from './containers/WalletContainer/PasswordBox'

const EnsureWalletPasswordContext = React.createContext<{
  ensureWalletPasswordInStore: (
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    callback: (...a: any[]) => void,
    overrides?: Overrides,
  ) => void
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
  const [overrides, setOverrides] = useState<Overrides | undefined>(undefined)

  const ensureWalletPasswordInStore = useCallback(
    (callback: () => void, actionOverrides?: Overrides) => {
      if (modalOpen) {
        return
      }

      if (!isParoleSet) {
        setOverrides(actionOverrides)
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
          overrides={overrides}
        />
      </Modal>
    </>
  )
}

export const useWithWalletPassword = (
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  action: (...args: any[]) => void,
  overrides?: Overrides,
) => {
  const { ensureWalletPasswordInStore } = useContext(
    EnsureWalletPasswordContext,
  )

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (...args: any[]) =>
    ensureWalletPasswordInStore(() => action(...args), overrides)
}
