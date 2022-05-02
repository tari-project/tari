import { ChangeEvent } from 'react'
import { useTheme } from 'styled-components'

import { useAppSelector, useAppDispatch } from '../../../store/hooks'
import { selectState as selectWalletState } from '../../../store/wallet/selectors'
import { actions as walletActions } from '../../../store/wallet'
import Tag from '../../../components/Tag'
import Box from '../../../components/Box'
import Loading from '../../../components/Loading'
import Text from '../../../components/Text'
import Button from '../../../components/Button'
import Link from '../../../components/Link'
import CopyBox from '../../../components/CopyBox'
import t from '../../../locales'
import { SettingsProps } from '../types'

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
      <Text type='header'>Wallet Settings</Text>
      <Box
        style={{
          borderRadius: 0,
          borderLeft: 'none',
          borderRight: 'none',
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          paddingLeft: 0,
          paddingRight: 0,
        }}
      >
        <span
          style={{
            display: 'flex',
            alignItems: 'baseline',
            columnGap: theme.spacingVertical(),
          }}
        >
          <Text>Wallet</Text>
          {running && (
            <Tag variant='small' type='running'>
              <span>{t.common.adjectives.running}</span>
            </Tag>
          )}
        </span>
        {running && (
          <Button variant='secondary' onClick={stop} loading={pending}>
            Stop
          </Button>
        )}
        {!running && (
          <Button onClick={start} loading={pending}>
            Start
          </Button>
        )}
      </Box>
      <CopyBox label='Tari Wallet ID (address)' value={address} />
      <Text type='smallMedium' color={theme.secondary}>
        Mined Tari is stored in Launchpad&apos;s wallet. Send funds to wallet of
        your choice (try{' '}
        <Link href='https://aurora.tari.com/'>Tari Aurora</Link> - it&apos;s
        great!) and enjoy extended functionality (including payment requests,
        recurring payments, ecommerce payments and more). To do this, you may
        need to convert the ID to emoji format.
      </Text>
    </>
  )
}

const WalletSettingsContainer = ({ onSettingsTouched }: SettingsProps) => {
  const dispatch = useAppDispatch()
  const { pending, running, address, unlocked } =
    useAppSelector(selectWalletState)
  const onChange = (event: ChangeEvent<HTMLInputElement>) => {
    const { checked } = event.target

    onSettingsTouched(checked)
  }

  if (!unlocked) {
    return <p>unlock wallet</p>
  }

  return (
    <WalletSettings
      running={running}
      pending={pending}
      stop={() => dispatch(walletActions.stop())}
      start={() => dispatch(walletActions.start())}
      address={address}
    />
  )
}

export default WalletSettingsContainer
