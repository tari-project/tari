import { TBotMessage } from '../components/TBot/TBotPrompt/types'
import introMessages from '../components/Onboarding/OnboardingMessages/IntroMessages'
import dockerInstallMessages, {
  DockerInstallDocs,
} from '../components/Onboarding/OnboardingMessages/DockerInstallMessages'
import {
  DownloadImagesMessage,
  DownloadImagesErrorMessage,
} from '../components/Onboarding/OnboardingMessages/DockerImagesMessages'
import { BlockchainSyncStep } from '../components/Onboarding/OnboardingMessages/LastStepsMessages'

export const OnboardingMessagesIntro: TBotMessage[] = [
  {
    content: introMessages[0],
    barFill: 0.0625,
    wait: 1000,
  },
  {
    content: introMessages[1],
    barFill: 0.125,
    wait: 2000,
  },
  {
    content: introMessages[2],
    barFill: 0.188,
    wait: 2000,
  },
  {
    content: introMessages[3],
    barFill: 0.25,
    wait: 2000,
    noSkip: true,
  },
]

export const OnboardingMessagesDockerInstall: (
  onDone: () => void,
) => TBotMessage[] = (onDone: () => void) => [
  {
    content: dockerInstallMessages[0],
    barFill: 0.3,
    wait: 2000,
  },
  {
    content: dockerInstallMessages[1],
    barFill: 0.35,
    wait: 2000,
  },
  {
    content: dockerInstallMessages[2],
    barFill: 0.4,
    wait: 2000,
  },
  {
    content: dockerInstallMessages[3],
    barFill: 0.45,
    wait: 2000,
  },
  {
    content: <DockerInstallDocs onDone={onDone} />,
    barFill: 0.5,
    wait: 2000,
    noSkip: true,
  },
]

export const OnboardingMessagesDockerInstallAfter: TBotMessage[] = [
  {
    content: dockerInstallMessages[4],
    barFill: 0.5,
    wait: 3000,
    noSkip: true,
  },
]

export { BlockchainSyncStep }
export { DownloadImagesMessage, DownloadImagesErrorMessage }
