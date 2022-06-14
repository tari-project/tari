/* eslint-disable react/jsx-key */
import Text from '../../Text'
import t from '../../../locales'

const messages = [
  <Text as='span' type='defaultHeavy'>
    {t.onboarding.message1.part1}{' '}
    <Text as='span' type='defaultMedium'>
      {t.onboarding.message1.part2}
    </Text>
  </Text>,
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
    return <Text>Message 5</Text>
  },
]

// const Message1 = (
//   <Text as='span' type='defaultHeavy'>
//     {t.onboarding.message1.part1}{' '}
//     <Text as='span' type='defaultMedium'>
//       {t.onboarding.message1.part2}
//     </Text>
//   </Text>
// )

// const Message2 = (
//   <Text as='span' type='defaultMedium'>
//     {t.onboarding.message2}
//   </Text>
// )

// const Message3 = (
//   <Text as='span' type='defaultMedium'>
//     {t.onboarding.message3}
//   </Text>
// )

// const Message4 = (
//   <Text as='span' type='defaultMedium'>
//     {t.onboarding.message4}
//   </Text>
// )

export default messages
