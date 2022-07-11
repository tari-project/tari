import { useState, SyntheticEvent } from 'react'
import { useTheme } from 'styled-components'

import PasswordInput from '../../components/Inputs/PasswordInput'
import Input from '../../components/Inputs/TextInput'
import Box from '../../components/Box'
import Text from '../../components/Text'
import Button from '../../components/Button'
import t from '../../locales'

import {
  SignetsContainer,
  TariBackgroundSignet,
  MoneroBackgroundSignet,
} from './styles'

import { WalletParole, MoneroCredentials } from './types'

const MINIMAL_PASSWORD_LENGTH = 4

const AllCredentialsBox = ({
  onSubmit,
  pending,
}: {
  onSubmit: (parole: WalletParole, monero: MoneroCredentials) => void
  pending?: boolean
}) => {
  const theme = useTheme()
  const [walletPassword, setWalletPassword] = useState('')
  const [moneroUsername, setMoneroUsername] = useState('')
  const [moneroPassword, setMoneroPassword] = useState('')

  const formSubmitHandler = (event: SyntheticEvent) => {
    event.preventDefault()

    onSubmit(walletPassword, {
      username: moneroUsername,
      password: moneroPassword,
    })
  }

  const disableSubmit =
    pending ||
    walletPassword.length < MINIMAL_PASSWORD_LENGTH ||
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
        <TariBackgroundSignet />
      </SignetsContainer>
      <div style={{ position: 'relative', zIndex: 1 }}>
        <Text type='header' style={{ marginBottom: theme.spacing() }}>
          {t.passwordPrompt.allCredentials.title}
        </Text>
        <Text>
          {t.passwordPrompt.scheduleCTA} {t.passwordPrompt.allCredentials.cta}
        </Text>
      </div>
      <form
        onSubmit={formSubmitHandler}
        style={{
          margin: `${theme.spacing()} 0`,
        }}
      >
        <PasswordInput
          label={
            <>
              <Text as='span' type='smallMedium'>
                1. {t.passwordPrompt.allCredentials.unlock}{' '}
              </Text>
              <Text as='span' type='smallHeavy'>
                {t.common.nouns.tariWallet}
              </Text>
            </>
          }
          autoFocus
          onChange={setWalletPassword}
          value={walletPassword}
          disabled={pending}
          placeholder={t.passwordPrompt.walletPasswordPlaceholder}
          useReveal
        />
        <Input
          label={
            <>
              <Text as='span' type='smallMedium'>
                2. {t.passwordPrompt.allCredentials.unlockExternal}{' '}
              </Text>
              <Text as='span' type='smallHeavy'>
                {t.common.nouns.moneroWallet}
              </Text>
            </>
          }
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

export default AllCredentialsBox
