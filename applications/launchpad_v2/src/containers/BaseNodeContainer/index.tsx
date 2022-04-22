import { useState } from 'react'
import { useTheme } from 'styled-components'

import Select from '../../components/Select'
import Text from '../../components/Text'
import Box from '../../components/Box'
import Button from '../../components/Button'
import t from '../../locales'

const networks = ['mainnet', 'testnet']
const networkOptions = networks.map(network => ({
  label: network,
  value: network,
  key: network,
}))

const BaseNodeContainer = () => {
  const theme = useTheme()
  const [tariNetwork, setTariNetwork] = useState(networkOptions[0])
  const [dark, setDark] = useState(false)
  const toggleDarkMode = () => setDark(a => !a)

  const startNode = () => console.log('start')
  const stopNode = () => console.log('stop')

  return (
    <>
      <button onClick={toggleDarkMode}>toggle dark mode</button>
      <Box
        border={!dark}
        gradient={
          dark
            ? { start: theme.actionBackground, end: theme.accent }
            : undefined
        }
      >
        <Text
          type='header'
          style={{ margin: 0 }}
          color={dark ? theme.inverted.primary : undefined}
        >
          {t.baseNode.title}
        </Text>
        <Box
          border={false}
          style={{ padding: 0, background: dark ? 'transparent' : undefined }}
        >
          <Select
            inverted={dark}
            value={tariNetwork}
            options={networkOptions}
            onChange={setTariNetwork}
            label={t.baseNode.tari_network_label}
          />
        </Box>
        {!dark && (
          <Button onClick={startNode}>
            <Text
              type='defaultMedium'
              color={theme.inverted.primary}
              style={{ lineHeight: '100%' }}
            >
              {t.baseNode.start}
            </Text>
          </Button>
        )}
        {dark && (
          <Button type='reset' onClick={stopNode}>
            <Text
              type='defaultMedium'
              color={theme.inverted.primary}
              style={{ lineHeight: '100%' }}
            >
              {t.common.verbs.stop}
            </Text>
          </Button>
        )}
      </Box>
    </>
  )
}

export default BaseNodeContainer
