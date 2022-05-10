import { SubmitHandler } from 'react-hook-form'

import { useAppDispatch } from '../../store/hooks'
import { actions as walletActions } from '../../store/wallet'

import { WalletPasswordWizardProps } from './types'
import WalletPasswordForm from './WalletPasswordForm'
import { WalletPasswordInputs } from './WalletPasswordForm/types'

/**
 * Wallet password form wired with Redux.
 * @param {string} [submitBtnText] - the text of the submit button.
 * @param {() => void} [onSuccess] - after the password is successfully set.
 *
 * @TODO - add handling exceptions in the `onSubmit` fnc after the wallet password logic
 * reaches the final form.
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
  const dispatch = useAppDispatch()

  const onSubmit: SubmitHandler<WalletPasswordInputs> = async data => {
    await dispatch(walletActions.unlockWallet(data.password))
    if (onSuccess) {
      onSuccess()
    }
  }

  return (
    <WalletPasswordForm onSubmit={onSubmit} submitBtnText={submitBtnText} />
  )
}

export default WalletPasswordWizardContainer
