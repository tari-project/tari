/* eslint-disable react/jsx-key */
import { useEffect } from 'react'
import Text from '../../Text'
import t from '../../../locales'
import Button from '../../Button'
import { useAppDispatch } from '../../../store/hooks'
import { setExpertView } from '../../../store/app'
import { setExpertSwitchDisabled } from '../../../store/app'

const messages = [
  () => {
    const dispatch = useAppDispatch()
    useEffect(() => {
      dispatch(setExpertSwitchDisabled(true))
    })
    return (
      <Text as='span' type='defaultHeavy'>
        {t.onboarding.message1.part1}{' '}
        <Text as='span' type='defaultMedium'>
          {t.onboarding.message1.part2}
        </Text>
      </Text>
    )
  },
  <Text as='span' type='defaultMedium'>
    {t.onboarding.message2}
  </Text>,
  <Text as='span' type='defaultMedium'>
    {t.onboarding.message3}
  </Text>,
  <Text as='span' type='defaultMedium'>
    {t.onboarding.message4}
  </Text>,
  () => {
    const dispatch = useAppDispatch()
    useEffect(() => {
      dispatch(setExpertSwitchDisabled(false))
    })
    return (
      <Text as='span' type='defaultMedium'>
        {t.onboarding.message5.part1}
        <Button
          variant='button-in-text'
          onClick={() => dispatch(setExpertView('open'))}
        >
          <Text as='span' type='defaultUnder'>
            {t.onboarding.message5.part2}
          </Text>
        </Button>
      </Text>
    )
  },
]

export default messages
