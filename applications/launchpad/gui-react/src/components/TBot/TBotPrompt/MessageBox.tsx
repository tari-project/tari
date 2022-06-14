import { forwardRef, ReactNode, ForwardedRef } from 'react'
import { useSpring } from 'react-spring'
import { useTheme } from 'styled-components'
import SvgArrowRight from '../../../styles/Icons/ArrowRight'
import Button from '../../Button'
import t from '../../../locales'
import {
  MessageSpaceContainer,
  StyledMessage,
  StyledMessageBox,
  MessageSlideIn,
  SkipButtonContainer,
} from './styles'

/**
 * Component renders the message wrapped with elements allowing to perform
 * fade-in animation.
 */
const MessageBox = (
  {
    animate,
    children,
    skipButton,
    floating,
  }: {
    animate: boolean
    children: ReactNode
    skipButton?: boolean
    floating?: boolean
  },
  ref?: ForwardedRef<HTMLDivElement>,
) => {
  const useOpacityAnim = useSpring({
    from: { opacity: animate ? 0 : 1 },
    to: { opacity: 1 },
    delay: 900,
  })

  const useSlideInAnim = useSpring({
    from: { top: animate ? 40 : 0 },
    to: { top: 0 },
    delay: 800,
  })

  const theme = useTheme()

  return (
    <StyledMessageBox ref={ref}>
      <StyledMessage
        style={{ opacity: 0 }}
        skipButton={skipButton}
        $floating={floating}
      >
        {children}
      </StyledMessage>
      <MessageSpaceContainer>
        <MessageSlideIn style={{ ...useSlideInAnim }}>
          <StyledMessage
            style={{ ...useOpacityAnim }}
            skipButton={skipButton}
            $floating={floating}
          >
            {children}
            {skipButton && (
              <SkipButtonContainer>
                <Button
                  style={{
                    textDecoration: 'none',
                    color: theme.secondary,
                  }}
                  variant='button-in-text'
                  rightIcon={<SvgArrowRight fontSize={24} />}
                  autosizeIcons={false}
                >
                  {t.onboarding.actions.skipChatting}
                </Button>
              </SkipButtonContainer>
            )}
          </StyledMessage>
        </MessageSlideIn>
      </MessageSpaceContainer>
    </StyledMessageBox>
  )
}

export default forwardRef(MessageBox)
