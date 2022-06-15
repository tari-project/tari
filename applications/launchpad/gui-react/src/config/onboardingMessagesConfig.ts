import { TBotMessages } from './../store/tbot/types'
import messages from '../components/Onboarding/OnboardingMessages'
import { TBotMessage } from '../components/TBot/TBotPrompt/types'

const OnBoardingMessagesConfig = {
  [TBotMessages.Onboarding]: [
    'onboardingMessage1',
    'onboardingMessage2',
    'onboardingMessage3',
  ],
}

export const OnboardingMessagesMap: TBotMessage[] = [
  {
    content: messages[0],
    barFill: 0.063,
    wait: 1000,
  },
  {
    content: messages[1],
    barFill: 0.125,
    wait: 5000,
  },
  {
    content: messages[2],
    barFill: 0.188,
    wait: 5000,
  },
  {
    content: messages[3],
    barFill: 0.25,
    wait: 5000,
  },
  {
    content: messages[4],
    barFill: 0.35,
    wait: 5000,
    noSkip: true,
  },
]

export default OnBoardingMessagesConfig
