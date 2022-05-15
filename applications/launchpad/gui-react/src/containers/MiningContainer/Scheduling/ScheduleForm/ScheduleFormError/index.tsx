import { useTheme } from 'styled-components'

import Button from '../../../../../components/Button'
import Text from '../../../../../components/Text'
import Backdrop from '../../../../../components/Backdrop'
import t from '../../../../../locales'
import Actions from '../../Actions'

import { ScheduleFormErrorWrapper } from './styles'

const ScheduleFormError = ({
  error,
  clearError,
}: {
  error: string | undefined
  clearError: () => void
}) => {
  const theme = useTheme()

  if (!error) {
    return null
  }

  return (
    <>
      <Backdrop opacity={0.15} borderRadius={theme.borderRadius()} />
      <ScheduleFormErrorWrapper>
        <Actions
          content={
            <>
              <Text as='span' type='smallHeavy' color={theme.warningDark}>
                {t.mining.scheduling.ops}
              </Text>{' '}
              <Text as='span' type='smallMedium' color={theme.warningDark}>
                {error}
              </Text>
            </>
          }
        >
          <Button variant='secondary' onClick={clearError}>
            {t.common.verbs.cancel}
          </Button>
          <Button
            style={{ flexGrow: 2, justifyContent: 'center' }}
            onClick={clearError}
          >
            {t.common.verbs.tryAgain}
          </Button>
        </Actions>
      </ScheduleFormErrorWrapper>
    </>
  )
}

export default ScheduleFormError
