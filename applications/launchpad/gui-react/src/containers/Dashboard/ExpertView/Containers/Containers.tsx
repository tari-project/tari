import { useTheme } from 'styled-components'

import Button from '../../../../components/Button'
import Text from '../../../../components/Text'
import Tag from '../../../../components/Tag'
import StopIcon from '../../../../styles/Icons/TurnOff'
import StartIcon from '../../../../styles/Icons/Play'
import t from '../../../../locales'

import { ServiceDto } from './types'
import { ContainersTable, TdRight } from './styles'

const Containers = ({ services }: { services: ServiceDto[] }) => {
  const theme = useTheme()

  return (
    <ContainersTable>
      {services.map(service => (
        <tr key={service.id}>
          <td>
            <Text color={theme.inverted.primary}>{service.name}</Text>
          </td>
          <TdRight>
            <Text color={theme.secondary} as='span'>
              {service.cpu}%
            </Text>{' '}
            <Text color={theme.secondary} as='span' type='smallMedium'>
              {t.common.nouns.cpu}
            </Text>
          </TdRight>
          <TdRight>
            <Text color={theme.secondary} as='span'>
              {service.memory}
            </Text>{' '}
            <Text color={theme.secondary} as='span' type='smallMedium'>
              {t.common.nouns.memory}
            </Text>
          </TdRight>
          <td>
            {service.running && (
              <Tag type='running' inverted style={{ margin: '0 auto' }}>
                {t.common.adjectives.running}
              </Tag>
            )}
          </td>
          <td style={{ minWidth: '75px' }}>
            {!service.running && (
              <Button
                variant='text'
                loading={service.pending}
                leftIcon={<StartIcon width='24px' height='24px' />}
                style={{
                  paddingRight: 0,
                  paddingLeft: 0,
                  color: theme.inverted.accentSecondary,
                }}
              >
                {t.common.verbs.start}
              </Button>
            )}
            {service.running && (
              <Button
                variant='text'
                loading={service.pending}
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
                {t.common.verbs.stop}
              </Button>
            )}
          </td>
        </tr>
      ))}
    </ContainersTable>
  )
}

export default Containers
