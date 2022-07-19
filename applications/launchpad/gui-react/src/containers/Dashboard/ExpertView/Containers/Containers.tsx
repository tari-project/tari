import { useState } from 'react'
import { useTheme } from 'styled-components'

import Button from '../../../../components/Button'
import Text from '../../../../components/Text'
import Tag from '../../../../components/Tag'
import Alert from '../../../../components/Alert'
import StopIcon from '../../../../styles/Icons/TurnOff'
import StartIcon from '../../../../styles/Icons/Play'
import t from '../../../../locales'

import { ContainersProps } from './types'
import { ContainersTable, TdRight } from './styles'

/**
 * @name Containers
 * @description Presentation component showing containers state
 *
 * @prop {ServiceDto[]} containers - containers which status should be displayed
 * @prop {(container: Container) => void} start - callback for starting a container
 * @prop {(containerId: ContainerId) => void} stop - callback for stopping a container
 *
 * @typedef ContainerDto
 * @prop {ContainerId} id - id of the container (if it is known)
 * @prop {Container} container - container which is described by this Dto
 * @prop {any} error - container or container type error
 * @prop {number} cpu - % cpu usage of the container
 * @prop {number} memory - memory in MB of the container
 * @prop {boolean} running - indicates if container is running
 * @prop {boolean} pending - indicates if container "running" state is about to change
 */
const Containers = ({ containers, stop, start }: ContainersProps) => {
  const theme = useTheme()
  const [error, setError] = useState('')

  return (
    <>
      <ContainersTable>
        <tbody>
          {containers.map(container => (
            <tr key={container.imageName}>
              <td>
                <Text color={theme.inverted.primary}>
                  {container.displayName}
                </Text>
              </td>
              <TdRight>
                <Text color={theme.secondary} as='span'>
                  {container.cpu.toFixed(2)}%
                </Text>{' '}
                <Text color={theme.secondary} as='span' type='smallMedium'>
                  {t.common.nouns.cpu}
                </Text>
              </TdRight>
              <TdRight>
                <Text color={theme.secondary} as='span'>
                  {container.memory.toFixed(2)} MB
                </Text>{' '}
                <Text color={theme.secondary} as='span' type='smallMedium'>
                  {t.common.nouns.memory}
                </Text>
              </TdRight>
              <td>
                {container.running && (
                  <Tag type='running' inverted style={{ margin: '0 auto' }}>
                    {t.common.adjectives.running}
                  </Tag>
                )}
                {container.error && (
                  <Button
                    variant='text'
                    style={{ margin: 0, padding: 0 }}
                    onClick={() => setError(container.error.toString())}
                  >
                    <Tag type='warning' inverted style={{ margin: '0 auto' }}>
                      {t.common.nouns.error}
                    </Tag>
                  </Button>
                )}
              </td>
              <td style={{ minWidth: '75px' }}>
                {!container.running && (
                  <Button
                    variant='text'
                    loading={container.pending}
                    autosizeIcons={false}
                    leftIcon={
                      <StartIcon
                        width='24px'
                        height='24px'
                        style={{ color: theme.inverted.accentSecondary }}
                      />
                    }
                    style={{
                      paddingRight: 0,
                      paddingLeft: 0,
                      color: theme.inverted.accentSecondary,
                    }}
                    onClick={() => start(container.container)}
                  >
                    <Text type='smallMedium'>{t.common.verbs.start}</Text>
                  </Button>
                )}
                {container.running && (
                  <Button
                    variant='text'
                    loading={container.pending}
                    autosizeIcons={false}
                    leftIcon={
                      <StopIcon
                        width='24px'
                        height='24px'
                        style={{ color: theme.secondary }}
                      />
                    }
                    style={{
                      paddingRight: 0,
                      paddingLeft: 0,
                      color: theme.placeholderText,
                    }}
                    onClick={() => stop(container.id)}
                  >
                    <Text type='smallMedium'>{t.common.verbs.stop}</Text>
                  </Button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </ContainersTable>
      <Alert
        open={Boolean(error)}
        content={error}
        onClose={() => setError('')}
      />
    </>
  )
}

export default Containers
