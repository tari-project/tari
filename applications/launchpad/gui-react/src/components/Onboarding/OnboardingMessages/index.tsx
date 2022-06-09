import Text from '../../Text'
import t from '../../../locales'
import { ReactNode } from 'react'

const Message1: ReactNode = <Text>{t.onboarding.message1.part1}</Text>

const Message2 = () => {
  return (
    <>
      <Text>{t.onboarding.message1.part2}</Text>
    </>
  )
}

export { Message1, Message2 }
