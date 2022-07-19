import { useState, SyntheticEvent } from 'react'
import { useTheme } from 'styled-components'

import PasswordInput from '../../components/Inputs/PasswordInput'
import Input from '../../components/Inputs/TextInput'
import Box from '../../components/Box'
import Text from '../../components/Text'
import Button from '../../components/Button'
import t from '../../locales'

import { SignetsContainer, MoneroBackgroundSignet } from './styles'

const MINIMAL_PASSWORD_LENGTH = 4

import { MoneroCredentials } from './types'

const MoneroCredentialsBox = ({
  onSubmit,
  pending,
}: {
  onSubmit: (monero: MoneroCredentials) => void
  pending?: boolean
}) => {
  const theme = useTheme()
  const [moneroUsername, setMoneroUsername] = useState('')
  const [moneroPassword, setMoneroPassword] = useState('')

  const formSubmitHandler = (event: SyntheticEvent) => {
    event.preventDefault()

    onSubmit({
      username: moneroUsername,
      password: moneroPassword,
    })
  }

  const disableSubmit =
    pending ||
    moneroPassword.length < MINIMAL_PASSWORD_LENGTH ||
    !moneroUsername

  return (
    <Box
      style={{
        position: 'relative',
        margin: 0,
        background: theme.nodeBackground,
        borderColor: theme.selectBorderColor,
      }}
    >
      <SignetsContainer>
        <MoneroBackgroundSignet />
      </SignetsContainer>
      <div style={{ position: 'relative', zIndex: 1 }}>
        <Text type='header' style={{ marginBottom: theme.spacing() }}>
          {t.passwordPrompt.moneroCredentials.title}
        </Text>
        <Text type='defaultMedium' as='span'>
          {t.passwordPrompt.scheduleCTA}{' '}
          {t.passwordPrompt.moneroCredentials.cta}
        </Text>
      </div>
      <form
        onSubmit={formSubmitHandler}
        style={{
          margin: `${theme.spacing()} 0`,
        }}
      >
        <Input
          onChange={setMoneroUsername}
          value={moneroUsername}
          disabled={pending}
          placeholder={t.passwordPrompt.moneroUsernamePlaceholder}
        />
        <PasswordInput
          onChange={setMoneroPassword}
          value={moneroPassword}
          disabled={pending}
          placeholder={t.passwordPrompt.moneroPasswordPlaceholder}
          useReveal
        />
        <Button
          disabled={disableSubmit}
          loading={pending}
          type='submit'
          style={{ marginTop: theme.spacing() }}
        >
          {t.common.verbs.continue}
        </Button>
      </form>
    </Box>
  )
}

export default MoneroCredentialsBox
