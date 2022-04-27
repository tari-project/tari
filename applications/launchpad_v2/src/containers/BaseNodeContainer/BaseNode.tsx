import { useTheme } from 'styled-components'

import Select from '../../components/Select'
import Text from '../../components/Text'
import Box from '../../components/Box'
import Button from '../../components/Button'
import Tag from '../../components/Tag'
import t from '../../locales'

import { BaseNodeProps, Network } from './types'

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
  pending,
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
        style={{
          margin: 0,
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
        }}
        color={running ? theme.inverted.primary : undefined}
      >
        {t.baseNode.title}
        {running && (
          <Tag type='running' variant='large'>
            {t.common.adjectives.running}
          </Tag>
        )}
      </Text>
      <Box
        border={false}
        style={{
          minWidth: 0,
          width: 'auto',
          padding: 0,
          background: running ? 'transparent' : undefined,
        }}
      >
        <Select
          inverted={running}
          disabled={running}
          value={networkOptions.find(({ value }) => value === tariNetwork)}
          options={networkOptions}
          onChange={({ value }) => setTariNetwork(value as Network)}
          label={t.baseNode.tari_network_label}
        />
      </Box>
      {!running && (
        <Button
          disabled={pending}
          onClick={startNode}
          loading={pending}
          style={{ color: theme.inverted.primary }}
        >
          <Text type='defaultMedium' style={{ color: theme.inverted.primary }}>
            {t.baseNode.start}
          </Text>
        </Button>
      )}
      {running && (
        <Button
          type='reset'
          onClick={stopNode}
          disabled={pending}
          loading={pending}
          style={{ color: theme.inverted.primary }}
        >
          <Text type='defaultMedium' color={theme.inverted.primary}>
            {t.common.verbs.stop}
          </Text>
        </Button>
      )}
    </Box>
  )
}

export default BaseNode
