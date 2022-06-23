/* eslint-disable react/jsx-key */
import { useEffect, useState } from 'react'
import { type } from '@tauri-apps/api/os'

import Text from '../../Text'
import t from '../../../locales'
import Button from '../../Button'

import LinksConfig from '../../../config/links'

const OS_NAMES = {
  Darwin: 'macOS',
  Windows_NT: 'Windows',
  Linux: 'Linux',
}

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
  () => {
    const [osName, setOsName] = useState('')

    const checkOs = async () => {
      const osType = await type()
      if (Object.keys(OS_NAMES).includes(osType)) {
        setOsName(OS_NAMES[osType as 'Darwin' | 'Windows_NT' | 'Linux'])
      }
    }

    useEffect(() => {
      checkOs()
    }, [])

    return (
      <Text as='span' type='defaultMedium'>
        {t.onboarding.dockerInstall.message3.part1} {osName}{' '}
        {t.onboarding.dockerInstall.message3.part2}{' '}
        <Text as='span' type='defaultHeavy'>
          {t.onboarding.dockerInstall.message3.part3}
        </Text>{' '}
        {t.onboarding.dockerInstall.message3.part4}
        &#128054;
      </Text>
    )
  },
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
