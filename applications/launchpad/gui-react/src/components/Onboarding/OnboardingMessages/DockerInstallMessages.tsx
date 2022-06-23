/* eslint-disable react/jsx-key */
import Text from '../../Text'
import t from '../../../locales'
import Button from '../../Button'

import LinksConfig from '../../../config/links'

const messages = [
  <Text as='span' type='defaultMedium'>
    {t.onboarding.dockerInstall.message1.part1}{' '}
    <Text as='span' type='defaultHeavy'>
      {t.onboarding.dockerInstall.message1.part2}
    </Text>{' '}
    {t.onboarding.dockerInstall.message1.part3}
  </Text>,
  <Text as='span' type='defaultMedium'>
    {t.onboarding.dockerInstall.message2}
  </Text>,
  <Text as='span' type='defaultMedium'>
    {t.onboarding.dockerInstall.message3.part1}{' '}
    <Text as='span' type='defaultHeavy'>
      {t.onboarding.dockerInstall.message3.part2}
    </Text>{' '}
    {t.onboarding.dockerInstall.message3.part3}
    &#128054;
  </Text>,
  <>
    <Text as='span' type='defaultMedium'>
      {t.onboarding.dockerInstall.message4.part1}
    </Text>
    <Button href={LinksConfig.discord}>
      {t.onboarding.dockerInstall.message4.part2}
    </Button>
  </>,
  <Text as='span' type='defaultMedium'>
    {t.onboarding.dockerInstall.afterInstall}
  </Text>,
]

/**
 * @TODO Fix view by ie. trying to embed the iframe - #23
 */
export const DockerInstallDocs = ({ onDone }: { onDone: () => void }) => {
  return (
    <div>
      <Text as='span' type='defaultMedium'>
        Docker docs
      </Text>
      <a
        href='https://docs.docker.com/get-docker/'
        target='_blank'
        rel='noreferrer'
      >
        Link to docker
      </a>
      <Button onClick={onDone}>Done</Button>
    </div>
  )
}

export default messages
