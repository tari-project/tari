import { useState, SyntheticEvent } from 'react'
import { useTheme } from 'styled-components'

import PasswordInput from '../../components/Inputs/PasswordInput'
import Box from '../../components/Box'
import Text from '../../components/Text'
import Button from '../../components/Button'
import t from '../../locales'

import { TariBackgroundSignet } from './styles'
import { WalletParole, Overrides } from './types'

const MINIMAL_PASSWORD_LENGTH = 4

const WalletPasswordBox = ({
  pending,
  onSubmit,
  overrides,
}: {
  pending: boolean
  onSubmit: (password: WalletParole) => void
  overrides?: Overrides
}) => {
  const theme = useTheme()
  const [walletPassword, setWalletPassword] = useState('')

  const formSubmitHandler = (event: SyntheticEvent) => {
    event.preventDefault()

    onSubmit(walletPassword)
  }

  const disableSubmit =
    pending || walletPassword.length < MINIMAL_PASSWORD_LENGTH

  return (
    <Box style={{ position: 'relative', margin: 0 }}>
      <TariBackgroundSignet />
      <div style={{ position: 'relative', zIndex: 1 }}>
        <Text type='header' style={{ marginBottom: theme.spacing() }}>
          {overrides?.title || t.wallet.password.title}
        </Text>
        <Text>{overrides?.cta || t.wallet.password.cta}</Text>
      </div>
      <form
        onSubmit={formSubmitHandler}
        style={{
          margin: `${theme.spacing()} 0`,
        }}
      >
        <PasswordInput
          autoFocus
          onChange={setWalletPassword}
          value={walletPassword}
          disabled={pending}
          placeholder={t.wallet.password.placeholderCta}
          useReveal
        />
        <Button disabled={disableSubmit} loading={pending} type='submit'>
          {t.common.verbs.continue}
        </Button>
      </form>
    </Box>
  )
}

export default WalletPasswordBox
