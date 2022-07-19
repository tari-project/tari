/* eslint-disable react/jsx-key */
import { useEffect, useRef, useState } from 'react'
import { type } from '@tauri-apps/api/os'

import Text from '../../Text'
import t from '../../../locales'
import Button from '../../Button'

import LinksConfig from '../../../config/links'
import { CtaButtonContainer } from './styles'
import { isDockerInstalled } from '../../../commands'
import { useAppDispatch } from '../../../store/hooks'
import { setOnboardingCheckpoint } from '../../../store/app'
import { OnboardingCheckpoints } from '../../../store/app/types'
import SvgDocker from '../../../styles/Icons/Docker'

const OS_NAMES = {
  Darwin: 'macOS',
  Windows_NT: 'Windows',
  Linux: 'Linux',
}

const DOCKER_DOCS_URLS = {
  Darwin: 'https://docs.docker.com/desktop/mac/install/',
  Windows_NT: 'https://docs.docker.com/desktop/windows/install/',
  Linux: 'https://docs.docker.com/engine/install/ubuntu/',
}

type OsType = keyof typeof OS_NAMES

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

    useEffect(() => {
      const checkOs = async () => {
        const osType = await type()
        if (Object.keys(OS_NAMES).includes(osType)) {
          setOsName(OS_NAMES[osType as OsType])
        }
      }

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

export const DockerInstallDocs = ({ onDone }: { onDone: () => void }) => {
  const dispatch = useAppDispatch()
  const intervalRef = useRef<ReturnType<typeof setInterval> | undefined>()

  const [docsUrl, setDocsUrl] = useState(DOCKER_DOCS_URLS.Linux)

  useEffect(() => {
    const checkOs = async () => {
      const osType = await type()
      if (Object.keys(DOCKER_DOCS_URLS).includes(osType)) {
        setDocsUrl(DOCKER_DOCS_URLS[osType as OsType])
      }
    }

    dispatch(setOnboardingCheckpoint(OnboardingCheckpoints.DOCKER_INSTALL))

    checkOs()
  }, [])

  useEffect(() => {
    // Wait until Docker is installed...
    intervalRef.current = setInterval(async () => {
      const isInstalled = await isDockerInstalled()
      if (isInstalled) {
        // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
        clearInterval(intervalRef.current!)
        onDone()
      }
    }, 5000)

    // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
    return () => clearInterval(intervalRef.current!)
  }, [])

  return (
    <>
      <CtaButtonContainer $noMargin>
        <Button variant='primary' href={docsUrl} leftIcon={<SvgDocker />}>
          {t.onboarding.dockerInstall.message5.link}
        </Button>
      </CtaButtonContainer>
    </>
  )
}

export default messages
