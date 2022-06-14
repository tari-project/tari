import { useState, useEffect } from 'react'
import { useTheme } from 'styled-components'
import TBotPrompt from '../../components/TBot/TBotPrompt'
import TitleBar from '../../components/TitleBar'
import { OnboardingMessagesMap } from '../../config/onboardingMessagesConfig'

/**
 * @example Example of trigger to push new messages
    <div key='key1'>
      <p>Push new message</p>
      <button onClick={() => fireTrigger(true)}>Push</button>
    </div>
 */

const Onboarding = () => {
  const theme = useTheme()
  const [trigger, fireTrigger] = useState(false)

  const [messages, setMessages] = useState(OnboardingMessagesMap)

  // If we use currentIndex = 1, it will not show loading dots before rendering first message
  const [current, setCurrent] = useState(1)

  useEffect(() => {
    if (trigger) {
      const newMsgs = messages.slice()
      newMsgs.push({ content: 'Newly pushed', barFill: 0 })
      setMessages(newMsgs)
      setCurrent(newMsgs.length - 1)
      const hideLoadingDots = () => {
        setCurrent(newMsgs.length)
      }
      hideLoadingDots()
      fireTrigger(false)
    }
  }, [trigger])

  return (
    <div>
      <TitleBar mode='onboarding' />
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          width: '100vw',
          height: '100%',
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: `${theme.backgroundSecondary}`,
        }}
      >
        <TBotPrompt
          open={true}
          messages={messages}
          currentIndex={current}
          closeIcon={false}
          mode='onboarding'
          floating={false}
        />
      </div>
    </div>
  )
}

export default Onboarding
