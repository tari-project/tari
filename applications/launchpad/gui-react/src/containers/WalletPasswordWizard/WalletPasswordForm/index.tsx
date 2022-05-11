import { useTheme } from 'styled-components'
import { useForm, Controller, SubmitHandler } from 'react-hook-form'

import Button from '../../../components/Button'
import Callout from '../../../components/Callout'
import Input from '../../../components/Inputs/Input'
import Text from '../../../components/Text'

import t from '../../../locales'

import { CalloutWrapper, FormButtons, WizardContainer } from './styles'
import { WalletPasswordInputs, WalletPasswordFormProps } from './types'

/**
 * Render the Wallet password form with text warnings.
 *
 * @param {string} [submitBtnText] - the text of the submit button.
 * @param {(data: WalletPasswordInputs) => Promise<void>} [onSubmit] - onform submit.
 */
const WalletPasswordForm = ({
  submitBtnText,
  onSubmit,
}: WalletPasswordFormProps) => {
  const theme = useTheme()

  const { control, handleSubmit, formState } = useForm<WalletPasswordInputs>({
    mode: 'onChange',
  })

  const onSubmitForm: SubmitHandler<WalletPasswordInputs> = async data => {
    await onSubmit(data)
  }

  return (
    <WizardContainer>
      <Text
        style={{ maxWidth: '75%', marginBottom: theme.spacingVertical(2.5) }}
        color={theme.primary}
      >
        {t.walletPasswordWizard.description}
      </Text>
      <Text type='smallMedium'>{t.walletPasswordWizard.warning}</Text>
      <CalloutWrapper>
        <Callout type='warning'>{t.walletPasswordWizard.warningBox}</Callout>
      </CalloutWrapper>
      <form onSubmit={handleSubmit(onSubmitForm)}>
        <Controller
          name='password'
          control={control}
          defaultValue=''
          rules={{ required: true }}
          render={({ field }) => (
            <Input
              placeholder={t.walletPasswordWizard.passwordPlaceholder}
              type='password'
              testId='password-input'
              {...field}
            />
          )}
        />

        <Text
          color={theme.primary}
          style={{ marginTop: theme.spacingVertical(0.62) }}
        >
          <span>{t.common.conjunctions.or} </span>
          <Button variant='button-in-text'>
            {t.walletPasswordWizard.generatePasswordBtn}
          </Button>
        </Text>
        <FormButtons>
          <Button
            type='submit'
            disabled={!formState.isValid || formState.isSubmitting}
            loading={formState.isSubmitting}
            testId='wallet-password-wizard-submit-btn'
          >
            {submitBtnText || t.walletPasswordWizard.submitBtn}
          </Button>
        </FormButtons>
      </form>
    </WizardContainer>
  )
}

export default WalletPasswordForm
