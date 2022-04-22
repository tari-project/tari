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

  const startNode = () => console.log('asdf')

  return (
    <Box>
      <Text type='header' style={{ margin: 0 }}>
        {t.baseNode.title}
      </Text>
      <Box border={false} style={{ padding: 0 }}>
        <Select
          value={tariNetwork}
          options={networkOptions}
          onChange={setTariNetwork}
          label={t.baseNode.tari_network_label}
        />
      </Box>
      <Button variant='primary' onClick={startNode}>
        <Text
          type='defaultMedium'
          color={theme.inverted.primary}
          style={{ lineHeight: '100%' }}
        >
          {t.baseNode.start}
        </Text>
      </Button>
    </Box>
  )
}

export default BaseNodeContainer
