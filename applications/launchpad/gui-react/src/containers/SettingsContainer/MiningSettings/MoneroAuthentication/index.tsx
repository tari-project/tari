import { Controller, SubmitHandler, useForm } from 'react-hook-form'
import { useTheme } from 'styled-components'
import Button from '../../../../components/Button'
import Input from '../../../../components/Inputs/Input'
import PasswordInput from '../../../../components/Inputs/PasswordInput'
import Text from '../../../../components/Text'

import t from '../../../../locales'
import { AuthenticationInputs } from '../../types'

import {
  Description,
  InputWrapper,
  ModalContainer,
  ModalContent,
  ModalFooter,
} from './styles'

/**
 * The Authentication form (username & password) dedicated to the Monero URL authentication.
 *
 * @param {AuthenticationInputs} [defaultValues] - initial values of `username` and `password`
 * @param {(data: AuthenticationInputs) => void} onSubmit - it is being called on the form submit
 * @param {(value: boolean) => void} close - cancel the form
 */
const MoneroAuthentication = ({
  defaultValues,
  onSubmit,
  close,
}: {
  defaultValues?: AuthenticationInputs
  onSubmit: (data: AuthenticationInputs) => void
  close: () => void
}) => {
  const theme = useTheme()
  const { control, handleSubmit } = useForm<AuthenticationInputs>({
    mode: 'onChange',
    defaultValues,
  })

  const onSubmitForm: SubmitHandler<AuthenticationInputs> = data => {
    onSubmit(data)
    close()
  }

  return (
    <ModalContainer
      style={{
        border: `1px solid ${theme.selectBorderColor}`,
        borderRadius: 'inherit',
      }}
    >
      <ModalContent>
        <Text as='h2' type='subheader' color={theme.primary}>
          {t.mining.settings.moneroAuthFormTitle}
        </Text>
        <Description>
          <Text type='smallMedium' color={theme.primary}>
            {t.mining.settings.moneroAuthFormDesc}
          </Text>
        </Description>

        <InputWrapper>
          <Controller
            name='username'
            control={control}
            render={({ field }) => (
              <Input
                placeholder={t.mining.settings.authUsernamePlaceholder}
                label={t.mining.settings.authUsernameLabel}
                testId='monero-auth-username-input'
                value={field.value?.toString()}
                onChange={v => field.onChange(v)}
                autoFocus
              />
            )}
          />
        </InputWrapper>

        <InputWrapper>
          <Controller
            name='password'
            control={control}
            render={({ field }) => (
              <PasswordInput
                placeholder={t.mining.settings.authPasswordPlaceholder}
                label={t.mining.settings.authPasswordLabel}
                testId='monero-auth-password-input'
                value={field.value?.toString()}
                onChange={v => field.onChange(v)}
                useReveal
              />
            )}
          />
        </InputWrapper>
      </ModalContent>
      <ModalFooter>
        <Button
          variant='secondary'
          size='small'
          onClick={close}
          testId='monero-auth-close-btn'
        >
          {t.common.verbs.cancel}
        </Button>
        <Button
          size='small'
          onClick={handleSubmit(onSubmitForm)}
          testId='monero-auth-submit-btn'
        >
          {t.common.verbs.submit}
        </Button>
      </ModalFooter>
    </ModalContainer>
  )
}

export default MoneroAuthentication
