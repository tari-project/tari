import React, { useCallback, useMemo, useState } from 'react'

import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { actions as settingsActions } from '../../store/settings'
import { selectIsParoleSet } from '../../store/settings/selectors'
import Modal from '../../components/Modal'
import PasswordBox, { Overrides } from '../WalletContainer/PasswordBox'

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

  const contextValue = useMemo(
    () => ({
      ensureWalletPasswordInStore,
    }),
    [ensureWalletPasswordInStore],
  )

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

export default PasswordsPrompt
