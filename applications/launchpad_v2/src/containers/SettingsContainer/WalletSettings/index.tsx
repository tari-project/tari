import { ChangeEvent } from 'react'
import { useTheme } from 'styled-components'

import Tag from '../../../components/Tag'
import Box from '../../../components/Box'
import Loading from '../../../components/Loading'
import Text from '../../../components/Text'
import Button from '../../../components/Button'
import Link from '../../../components/Link'
import CopyBox from '../../../components/CopyBox'
import t from '../../../locales'
import { SettingsProps } from '../types'

const address = '7a6ffed9-4252-427e-af7d-3dcaaf2db2df'

const WalletSettings = ({ onSettingsTouched }: SettingsProps) => {
  const theme = useTheme()

  const onChange = (event: ChangeEvent<HTMLInputElement>) => {
    const { checked } = event.target

    onSettingsTouched(checked)
  }

  const running = true
  const pending = false

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
          paddingLeft: 0,
          paddingRight: 0,
        }}
      >
        <span
          style={{
            display: 'flex',
            alignItems: 'center',
            columnGap: theme.spacingVertical(),
          }}
        >
          <Text>Wallet</Text>
          {running && !pending ? (
            <Tag variant='small' type='running'>
              <span>{t.common.adjectives.running}</span>
            </Tag>
          ) : null}
          {pending ? <Loading loading={true} size='12px' /> : null}
        </span>
        <Button variant='secondary'>Stop</Button>
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

export default WalletSettings
