import { forwardRef, ReactNode, ForwardedRef } from 'react'
import { useSpring } from 'react-spring'
import {
  MessageSpaceContainer,
  StyledMessage,
  StyledMessageBox,
  MessageSlideIn,
} from './styles'

/**
 * Component renders the message wrapped with elements allowing to perform
 * fade-in animation.
 */
const MessageBox = (
  {
    animate,
    children,
  }: {
    animate: boolean
    children: ReactNode
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

  return (
    <StyledMessageBox ref={ref}>
      <StyledMessage style={{ opacity: 0 }}>{children}</StyledMessage>
      <MessageSpaceContainer>
        <MessageSlideIn style={{ ...useSlideInAnim }}>
          <StyledMessage style={{ ...useOpacityAnim }}>
            {children}
          </StyledMessage>
        </MessageSlideIn>
      </MessageSpaceContainer>
    </StyledMessageBox>
  )
}

export default forwardRef(MessageBox)
