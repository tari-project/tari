import { CSSProperties, useState, SyntheticEvent } from 'react'
import { useTheme } from 'styled-components'

import PasswordInput from '../../components/Inputs/PasswordInput'
import Box from '../../components/Box'
import Text from '../../components/Text'
import Button from '../../components/Button'
import t from '../../locales'

import { TariBackgroundSignet } from './styles'
import { WalletPasswordConfirmationStatus } from '../../store/temporary/types'
import { useAppSelector } from '../../store/hooks'
import { selectTheme } from '../../store/app/selectors'

const MINIMAL_PASSWORD_LENGTH = 4

const PasswordBox = ({
  pending,
  passwordConfirmStatus,
  onSubmit,
  style,
}: {
  pending: boolean
  passwordConfirmStatus?: WalletPasswordConfirmationStatus
  onSubmit: (password: string) => void
  style?: CSSProperties
}) => {
  const theme = useTheme()
  const currentTheme = useAppSelector(selectTheme)
  const [password, setPassword] = useState('')
  const updatePassword = (v: string) => {
    setPassword(v)
  }

  const formSubmitHandler = (event: SyntheticEvent) => {
    event.preventDefault()

    onSubmit(password)
  }

  const disableSubmit = pending || password.length < MINIMAL_PASSWORD_LENGTH
  const failed =
    passwordConfirmStatus === 'failed' ||
    passwordConfirmStatus === 'wrong_password'

  return (
    <Box style={{ position: 'relative', ...style }}>
      <TariBackgroundSignet
        style={{ opacity: currentTheme === 'light' ? 1 : 0.2 }}
      />
      <div style={{ position: 'relative', zIndex: 1 }}>
        <Text type='header' style={{ marginBottom: theme.spacing() }}>
          {t.wallet.password.title}
        </Text>
        <Text>{t.wallet.password.cta}</Text>
      </div>
      <form onSubmit={formSubmitHandler}>
        <PasswordInput
          autoFocus
          onChange={updatePassword}
          value={password}
          disabled={pending && !failed}
          placeholder={t.wallet.password.placeholderCta}
          containerStyle={{
            margin: `${theme.spacing()} 0`,
          }}
          useReveal
        />
        {passwordConfirmStatus === 'wrong_password' && (
          <Text
            color={theme.error}
            style={{ marginBottom: theme.spacingVertical(1) }}
          >
            {t.wallet.theEnteredPasswordIsIncorrect}
          </Text>
        )}
        {passwordConfirmStatus === 'failed' && (
          <Text
            color={theme.error}
            style={{ marginBottom: theme.spacingVertical(1) }}
          >
            {t.common.phrases.somethingWentWrong}
          </Text>
        )}
        <Button
          disabled={disableSubmit && !failed}
          loading={pending && !failed}
          type='submit'
        >
          {failed ? t.common.verbs.tryAgain : t.common.verbs.continue}
        </Button>
      </form>
    </Box>
  )
}

export default PasswordBox
