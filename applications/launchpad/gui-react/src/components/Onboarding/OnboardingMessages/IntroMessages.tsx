/* eslint-disable react/jsx-key */
import { useEffect } from 'react'
import Text from '../../Text'
import t from '../../../locales'
import { useAppDispatch } from '../../../store/hooks'
import { setExpertSwitchDisabled } from '../../../store/app'

const messages = [
  () => {
    const dispatch = useAppDispatch()
    useEffect(() => {
      dispatch(setExpertSwitchDisabled(true))
    })
    return (
      <Text as='span' type='defaultHeavy'>
        {t.onboarding.intro.message1.part1}{' '}
        <Text as='span' type='defaultMedium'>
          {t.onboarding.intro.message1.part2}
        </Text>
      </Text>
    )
  },
  <Text as='span' type='defaultMedium'>
    {t.onboarding.intro.message2}
  </Text>,
  <Text as='span' type='defaultMedium'>
    {t.onboarding.intro.message3}
  </Text>,
  <Text as='span' type='defaultMedium'>
    {t.onboarding.intro.message4}
  </Text>,
]

export default messages
