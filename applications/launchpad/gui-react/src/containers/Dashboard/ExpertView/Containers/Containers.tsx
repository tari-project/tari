import { useTheme } from 'styled-components'

import Button from '../../../../components/Button'
import Text from '../../../../components/Text'
import Tag from '../../../../components/Tag'
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
 * @prop {Container} container - container which is described by this Dto
 * @prop {number} cpu - % cpu usage of the container
 * @prop {number} memory - memory in MB of the container
 * @prop {boolean} running - indicates if container is running
 * @prop {boolean} pending - indicates if container "running" state is about to change
 */
const Containers = ({ containers, stop, start }: ContainersProps) => {
  const theme = useTheme()

  return (
    <ContainersTable>
      <tbody>
        {containers.map(container => (
          <tr key={container.container}>
            <td>
              <Text color={theme.inverted.primary}>
                {t.common.containers[container.container]}
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
            </td>
            <td style={{ minWidth: '75px' }}>
              {!container.running && (
                <Button
                  variant='text'
                  loading={container.pending}
                  leftIcon={<StartIcon width='24px' height='24px' />}
                  style={{
                    paddingRight: 0,
                    paddingLeft: 0,
                    color: theme.inverted.accentSecondary,
                  }}
                  onClick={() => start(container.container)}
                >
                  {t.common.verbs.start}
                </Button>
              )}
              {container.running && (
                <Button
                  variant='text'
                  loading={container.pending}
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
                    color: theme.inverted.primary,
                  }}
                  onClick={() => stop(container.id)}
                >
                  {t.common.verbs.stop}
                </Button>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </ContainersTable>
  )
}

export default Containers
