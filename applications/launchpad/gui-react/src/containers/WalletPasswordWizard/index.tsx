import { useState } from 'react'
import { SubmitHandler } from 'react-hook-form'

import { useAppDispatch } from '../../store/hooks'
import { actions as walletActions } from '../../store/wallet'
import Alert from '../../components/Alert'
import { actions as credentialsActions } from '../../store/credentials'

import { WalletPasswordWizardProps } from './types'
import WalletPasswordForm from './WalletPasswordForm'
import { WalletPasswordInputs } from './WalletPasswordForm/types'

/**
 * Wallet password form wired with Redux.
 * @prop {string} [submitBtnText] - the text of the submit button.
 * @prop {() => void} [onSuccess] - after the password is successfully set.
 *
 * @example
 * <WalletPasswordWizard
 *   submitBtnText='Set password'
 *   onSuccess={() => dispatch(actions.runNode())}
 * />
 */
const WalletPasswordWizardContainer = ({
  submitBtnText,
  onSuccess,
}: WalletPasswordWizardProps) => {
  const [error, setError] = useState('')
  const dispatch = useAppDispatch()

  const onSubmit: SubmitHandler<WalletPasswordInputs> = async data => {
    try {
      dispatch(credentialsActions.setWallet(data.password))
      await dispatch(walletActions.unlockWallet()).unwrap()
      if (onSuccess) {
        onSuccess()
      }
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
    } catch (e: any) {
      setError(e)
    }
  }

  return (
    <>
      <WalletPasswordForm onSubmit={onSubmit} submitBtnText={submitBtnText} />
      <Alert
        title='Error'
        open={Boolean(error)}
        onClose={() => setError('')}
        content={error}
      />
    </>
  )
}

export default WalletPasswordWizardContainer
