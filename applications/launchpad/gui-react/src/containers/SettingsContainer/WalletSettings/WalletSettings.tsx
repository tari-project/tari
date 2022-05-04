import { useTheme } from 'styled-components'

import Tag from '../../../components/Tag'
import Text from '../../../components/Text'
import Button from '../../../components/Button'
import CopyBox from '../../../components/CopyBox'
import t from '../../../locales'

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

  return (
    <>
      <Text type='header'>{t.wallet.settings.title}</Text>
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
          <Button onClick={start} loading={pending}>
            {t.common.verbs.start}
          </Button>
        )}
      </IsWalletRunningRow>
      <CopyBox
        label={`${t.wallet.wallet.walletId} (${t.wallet.wallet.address})`}
        value={address}
      />
      <Text type='smallMedium' color={theme.secondary}>
        {t.wallet.settings.explanations.storage}{' '}
        {t.wallet.settings.explanations.send} (
        {t.wallet.settings.explanations.try}{' '}
        <Button href='https://aurora.tari.com/'>
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
