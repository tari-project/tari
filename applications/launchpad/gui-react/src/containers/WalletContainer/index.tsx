import { useState } from 'react'
import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { actions } from '../../store/wallet'
import {
  selectIsUnlocked,
  selectIsPending,
  selectWalletSetupRequired,
  selectIsRunning,
} from '../../store/wallet/selectors'
import { WalletSetupRequired } from '../../store/wallet/types'
import { actions as credentialsActions } from '../../store/credentials'
import CenteredLayout from '../../components/CenteredLayout'

import WalletContent from './WalletContent'
import PasswordBox from './PasswordBox'
import WalletSetupBox from './WalletSetupBox'
import { selectWallet } from '../../store/credentials/selectors'
import Alert from '../../components/Alert'
import { selectWalletPasswordConfirmation } from '../../store/temporary/selectors'
import { temporaryActions } from '../../store/temporary'
import { useTheme } from 'styled-components'

const WalletContainer = () => {
  const dispatch = useAppDispatch()
  const theme = useTheme()

  const unlocked = useAppSelector(selectIsUnlocked)
  const pending = useAppSelector(selectIsPending)
  const running = useAppSelector(selectIsRunning)
  const walletSetupRequired = useAppSelector(selectWalletSetupRequired)
  const walletCredentials = useAppSelector(selectWallet)
  const walletPassConfirm = useAppSelector(selectWalletPasswordConfirmation)

  const [error, setError] = useState('')

  if (walletSetupRequired === WalletSetupRequired.MissingWalletAddress) {
    return (
      <CenteredLayout horizontally>
        <WalletSetupBox />
      </CenteredLayout>
    )
  }

  if (
    !unlocked ||
    !walletCredentials ||
    walletPassConfirm === 'waiting_for_confirmation' ||
    walletPassConfirm === 'failed' ||
    walletPassConfirm === 'wrong_password'
  ) {
    return (
      <CenteredLayout horizontally vertically>
        <PasswordBox
          pending={
            (pending && !running) ||
            walletPassConfirm === 'waiting_for_confirmation'
          }
          passwordConfirmStatus={walletPassConfirm}
          onSubmit={async password => {
            try {
              dispatch(
                temporaryActions.setWalletPasswordConfirmation(
                  'waiting_for_confirmation',
                ),
              )
              dispatch(credentialsActions.setWallet(password))
              await dispatch(actions.unlockWallet()).unwrap()
            } catch (err) {
              setError((err as Error).toString())
            }
          }}
          style={{ background: theme.nodeBackground, color: theme.helpTipText }}
        />
        <Alert
          title='Error'
          open={Boolean(error)}
          onClose={() => setError('')}
          content={error}
        />
      </CenteredLayout>
    )
  }

  return (
    <CenteredLayout horizontally>
      <WalletContent />
    </CenteredLayout>
  )
}

export default WalletContainer
