import { useTheme } from 'styled-components'

import Tag from '../../../components/Tag'
import Text from '../../../components/Text'
import Button from '../../../components/Button'
import CopyBox from '../../../components/CopyBox'
import t from '../../../locales'
import useWithPasswordPrompt from '../../../containers/PasswordPrompt/useWithPasswordPrompt'

import { IsWalletRunningRow, WalletRunningContainer } from './styles'

const WalletSettings = ({
  running,
  pending,
  address,
  stop,
  start,
}: {
  running: boolean
  pending: boolean
  address: string
  stop: () => void
  start: () => void
}) => {
  const theme = useTheme()
  const startWallet = useWithPasswordPrompt(start, { wallet: true })

  return (
    <>
      <Text type='subheader' as='h2' color={theme.primary}>
        {t.wallet.settings.title}
      </Text>
      <IsWalletRunningRow>
        <WalletRunningContainer>
          <Text>{t.common.nouns.wallet}</Text>
          {running && (
            <Tag variant='small' type='running'>
              <span>{t.common.adjectives.running}</span>
            </Tag>
          )}
        </WalletRunningContainer>
        {running && (
          <Button variant='secondary' onClick={stop} loading={pending}>
            {t.common.verbs.stop}
          </Button>
        )}
        {!running && (
          <Button onClick={startWallet} loading={pending}>
            {t.common.verbs.start}
          </Button>
        )}
      </IsWalletRunningRow>
      <CopyBox
        label={`${t.wallet.wallet.walletId} (${t.wallet.wallet.address})`}
        labelColor={theme.primary}
        value={address}
        style={{
          background: theme.settingsCopyBoxBackground,
          borderColor: theme.selectBorderColor,
        }}
      />
      <Text type='smallMedium' color={theme.nodeWarningText}>
        {t.wallet.settings.explanations.storage}{' '}
        {t.wallet.settings.explanations.send} (
        {t.wallet.settings.explanations.try}{' '}
        <Button href='https://aurora.tari.com/' size='small'>
          {t.wallet.settings.explanations.aurora}
        </Button>{' '}
        - {t.wallet.settings.explanations.itsGreat}){' '}
        {t.wallet.settings.explanations.extendedFunctionality}{' '}
        {t.wallet.settings.explanations.convert}
      </Text>
    </>
  )
}

export default WalletSettings
