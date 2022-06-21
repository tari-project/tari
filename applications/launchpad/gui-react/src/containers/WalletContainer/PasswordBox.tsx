import { CSSProperties, useState, SyntheticEvent } from 'react'
import { useTheme } from 'styled-components'

import PasswordInput from '../../components/Inputs/PasswordInput'
import Box from '../../components/Box'
import Text from '../../components/Text'
import Button from '../../components/Button'
import t from '../../locales'

import { TariBackgroundSignet } from './styles'

const MINIMAL_PASSWORD_LENGTH = 4

const PasswordBox = ({
  pending,
  onSubmit,
  style,
}: {
  pending: boolean
  onSubmit: (password: string) => void
  style?: CSSProperties
}) => {
  const theme = useTheme()
  const [password, setPassword] = useState('')
  const updatePassword = (v: string) => {
    setPassword(v)
  }

  const formSubmitHandler = (event: SyntheticEvent) => {
    event.preventDefault()

    onSubmit(password)
  }

  const disableSubmit = pending || password.length < MINIMAL_PASSWORD_LENGTH

  return (
    <Box style={{ position: 'relative', ...style }}>
      <TariBackgroundSignet />
      <Text type='header' style={{ marginBottom: theme.spacing() }}>
        {t.wallet.password.title}
      </Text>
      <Text>{t.wallet.password.cta}</Text>
      <form onSubmit={formSubmitHandler}>
        <PasswordInput
          autoFocus
          onChange={updatePassword}
          value={password}
          disabled={pending}
          placeholder={t.wallet.password.placeholderCta}
          containerStyle={{
            margin: `${theme.spacing()} 0`,
          }}
          useReveal
        />
        <Button disabled={disableSubmit} loading={pending} type='submit'>
          {t.common.verbs.continue}
        </Button>
      </form>
    </Box>
  )
}

export default PasswordBox
