import { useTheme } from 'styled-components'

import Select from '../../components/Select'
import Text from '../../components/Text'
import Box from '../../components/Box'
import Button from '../../components/Button'
import t from '../../locales'

import { BaseNodeProps } from './types'

const networks = ['mainnet', 'testnet']
const networkOptions = networks.map(network => ({
  label: network,
  value: network,
  key: network,
}))

const BaseNode = ({
  startNode,
  stopNode,
  running,
  tariNetwork,
  setTariNetwork,
}: BaseNodeProps) => {
  const theme = useTheme()

  return (
    <Box
      border={!running}
      gradient={
        running
          ? { start: theme.actionBackground, end: theme.accent }
          : undefined
      }
    >
      <Text
        type='header'
        style={{ margin: 0 }}
        color={running ? theme.inverted.primary : undefined}
      >
        {t.baseNode.title}
      </Text>
      <Box
        border={false}
        style={{ padding: 0, background: running ? 'transparent' : undefined }}
      >
        <Select
          inverted={running}
          value={networkOptions.find(({ value }) => value === tariNetwork)}
          options={networkOptions}
          onChange={({ value }) => setTariNetwork(value)}
          label={t.baseNode.tari_network_label}
        />
      </Box>
      {!running && (
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
      {running && (
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
  )
}

export default BaseNode
