import { useTheme } from 'styled-components'

import Select from '../../components/Select'
import Text from '../../components/Text'
import Box from '../../components/Box'
import Button from '../../components/Button'
import Tag from '../../components/Tag'
import t from '../../locales'

import { BaseNodeProps, Network } from './types'
import { networkOptions } from './constants'

const BaseNode = ({
  running,
  pending,
  startNode,
  stopNode,
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
        <Button disabled={pending} onClick={startNode} loading={pending}>
          {t.baseNode.start}
        </Button>
      )}
      {running && (
        <Button
          onClick={stopNode}
          disabled={pending}
          loading={pending}
          style={{
            color: theme.inverted.primary,
            background: theme.resetBackground,
            border: 'none',
          }}
        >
          {t.common.verbs.stop}
        </Button>
      )}
    </Box>
  )
}

export default BaseNode
