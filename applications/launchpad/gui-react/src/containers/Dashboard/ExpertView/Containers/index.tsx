import { useTheme } from 'styled-components'

import Button from '../../../../components/Button'
import Text from '../../../../components/Text'
import Tag from '../../../../components/Tag'
import StopIcon from '../../../../styles/Icons/TurnOff'
import StartIcon from '../../../../styles/Icons/Play'

const Containers = ({ services }: { services: any[] }) => {
  const theme = useTheme()

  return (
    <table
      style={{ width: '100%', maxWidth: '50vw', marginTop: theme.spacing() }}
    >
      {services.map(service => (
        <tr key={service.id}>
          <td>
            <Text color={theme.inverted.primary}>{service.name}</Text>
          </td>
          <td style={{ textAlign: 'right' }}>
            <Text color={theme.secondary} as='span'>
              {service.cpu}%
            </Text>{' '}
            <Text color={theme.secondary} as='span' type='smallMedium'>
              CPU
            </Text>
          </td>
          <td style={{ textAlign: 'right' }}>
            <Text color={theme.secondary} as='span'>
              {service.memory}
            </Text>{' '}
            <Text color={theme.secondary} as='span' type='smallMedium'>
              Memory
            </Text>
          </td>
          <td>
            {service.running && (
              <Tag type='running' inverted style={{ margin: '0 auto' }}>
                Running
              </Tag>
            )}
          </td>
          <td>
            {!service.running && (
              <Button
                variant='text'
                leftIcon={<StartIcon width='24px' height='24px' />}
                style={{
                  paddingRight: 0,
                  paddingLeft: 0,
                  color: theme.inverted.accentSecondary,
                }}
              >
                Start
              </Button>
            )}
            {service.running && (
              <Button
                variant='text'
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
              >
                Stop
              </Button>
            )}
          </td>
        </tr>
      ))}
    </table>
  )
}

const ContainersContainer = () => {
  const services = [
    {
      id: 'asdflksajdflkasjdf',
      name: 'Tor',
      cpu: 1.2,
      memory: '8 MB',
      running: true,
    },
    {
      id: 'oiausdofiasdofiu',
      name: 'Base Node',
      cpu: 2,
      memory: '12 MB',
      running: false,
    },
  ]

  return <Containers services={services} />
}

export default ContainersContainer
