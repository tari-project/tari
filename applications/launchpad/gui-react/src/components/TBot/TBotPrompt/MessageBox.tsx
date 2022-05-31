import { forwardRef, ReactNode, ForwardedRef } from 'react'
import { animated, useSpring } from 'react-spring'
import { StyledMessage } from './styles'

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
  const anim = useSpring({
    from: { opacity: animate ? 0 : 1 },
    to: { opacity: 1 },
    delay: 900,
  })

  const anim2 = useSpring({
    from: { top: animate ? 40 : 0 },
    to: { top: 0 },
    delay: 800,
  })

  return (
    <div ref={ref} style={{ position: 'relative' }}>
      <StyledMessage style={{ opacity: 0 }}>{children}</StyledMessage>
      <div
        style={{
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          position: 'absolute',
          overflow: 'hidden',
        }}
      >
        <animated.div
          style={{
            ...anim2,
            position: 'absolute',
            left: 0,
            right: 0,
          }}
        >
          <StyledMessage
            style={{
              ...anim,
            }}
          >
            {children}
          </StyledMessage>
        </animated.div>
      </div>
    </div>
  )
}

export default forwardRef(MessageBox)
