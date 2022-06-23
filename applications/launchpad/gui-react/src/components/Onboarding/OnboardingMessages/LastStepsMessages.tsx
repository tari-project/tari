/* eslint-disable react/jsx-key */
import Text from '../../Text'
import t from '../../../locales'
import Button from '../../Button'
import { useAppDispatch } from '../../../store/hooks'
import { setOnboardingComplete } from '../../../store/app'

/**
 * @TODO - how the Blockchain synchronization should to work?
 */
const messages = [
  () => {
    const dispatch = useAppDispatch()

    return (
      <>
        <Text as='span' type='defaultHeavy'>
          {t.onboarding.lastSteps.message1} ✨💪
        </Text>
        <Button
          variant='button-in-text'
          onClick={() => dispatch(setOnboardingComplete(true))}
        >
          <Text as='span' type='defaultUnder'>
            Exit data synchronization
          </Text>
        </Button>
      </>
    )
  },
]

export default messages
