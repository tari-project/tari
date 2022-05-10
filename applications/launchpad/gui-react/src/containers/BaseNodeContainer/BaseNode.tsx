import { useTheme } from 'styled-components'

import Select from '../../components/Select'
import Text from '../../components/Text'
import Box from '../../components/Box'
import Button from '../../components/Button'
import Tag from '../../components/Tag'
import Callout from '../../components/Callout'
import Capitalize from '../../components/Capitalize'
import CenteredLayout from '../../components/CenteredLayout'
import t from '../../locales'

import { BaseNodeProps, Network } from './types'

const networks = ['dibbler', 'testnet']
const networkOptions = networks.map(network => ({
  label: network,
  value: network,
  key: network,
}))

const BaseNode = ({
  startNode,
  stopNode,
  openExpertView,
  containers,
  tariNetwork,
  setTariNetwork,
}: BaseNodeProps) => {
  const theme = useTheme()

  const pending = containers.some(container => container.pending)
  const notRunningContainers = containers.filter(
    container => !container.running,
  )
  const runningContainers = containers.filter(container => container.running)
  const running = Boolean(runningContainers.length)

  const unhealthy =
    containers.some(container => !container.pending && container.running) &&
    containers.some(container => !container.pending && !container.running)

  return (
    <CenteredLayout horizontally>
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
          {running && !unhealthy && (
            <Tag type='running' variant='large'>
              {t.common.adjectives.running}
            </Tag>
          )}
          {unhealthy && (
            <Tag type='warning' variant='large'>
              Unhealthy
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
            <Text type='defaultMedium'>{t.baseNode.start}</Text>
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
            <Text type='defaultMedium'>{t.common.verbs.stop}</Text>
          </Button>
        )}
        {unhealthy && (
          <div style={{ marginTop: theme.spacing() }}>
            <Callout type='warning'>
              <Capitalize>{t.common.adjectives.only}</Capitalize>{' '}
              <strong>{runningContainers.length}</strong>{' '}
              {t.baseNode.unhealthy.ofTheRequired}{' '}
              {t.baseNode.unhealthy.containers}
              <br />
              {notRunningContainers.map((c, index, arr) => (
                <em key={c.type}>
                  {t.common.containers[c.type]}
                  {index < arr.length - 1 ? ', ' : ''}
                </em>
              ))}
              <br />
              {t.baseNode.unhealthy.checkTheirState}{' '}
              <Button
                variant='text'
                style={{ display: 'inline-block', padding: 0 }}
                onClick={openExpertView}
              >
                {t.common.nouns.expertView}
              </Button>{' '}
              {t.baseNode.unhealthy.bringItDown}
            </Callout>
          </div>
        )}
      </Box>
    </CenteredLayout>
  )
}

export default BaseNode
