import { useTheme } from 'styled-components'
import { useState, ChangeEvent, SyntheticEvent } from 'react'

import Box from '../../components/Box'
import Text from '../../components/Text'
import Button from '../../components/Button'
import t from '../../locales'

import { TariBackgroundSignet } from './styles'

const MINIMAL_PASSWORD_LENGTH = 5

const PasswordBox = ({
  pending,
  onSubmit,
}: {
  pending: boolean
  onSubmit: (password: string) => void
}) => {
  const theme = useTheme()
  const [password, setPassword] = useState('')
  const updatePassword = (event: ChangeEvent<HTMLInputElement>) => {
    const v = event.target.value

    setPassword(v)
  }

  const formSubmitHandler = (event: SyntheticEvent) => {
    event.preventDefault()

    onSubmit(password)
    setPassword('')
  }

  const disableSubmit = pending || password.length < MINIMAL_PASSWORD_LENGTH

  return (
    <Box style={{ position: 'relative' }}>
      <TariBackgroundSignet />
      <Text type='header' style={{ marginBottom: theme.spacing() }}>
        {t.wallet.password.title}
      </Text>
      <Text>{t.wallet.password.cta}</Text>
      <form onSubmit={formSubmitHandler}>
        <input
          type='password'
          onChange={updatePassword}
          value={password}
          disabled={pending}
          placeholder={t.wallet.password.placeholderCta}
          style={{
            marginTop: theme.spacingVertical(3),
            marginBottom: theme.spacingVertical(1.5),
            width: '100%',
          }}
        />
        <Button
          disabled={disableSubmit}
          loading={pending}
          type='submit'
          variant={disableSubmit ? 'disabled' : undefined}
        >
          <Text type='defaultMedium' style={{ lineHeight: '100%' }}>
            {t.common.verbs.continue}
          </Text>
        </Button>
      </form>
    </Box>
  )
}

export default PasswordBox
