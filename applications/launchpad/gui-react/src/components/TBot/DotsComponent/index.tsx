import { useEffect, useRef } from 'react'
import lottie from 'lottie-web'
import dotsChatLottieLight from '../../../styles/lotties/tbot-dots-animation-light.json'
import dotsChatLottieDark from '../../../styles/lotties/tbot-dots-animation-dark.json'
import { DotsContainer, StyledRow } from './styles'

/**
 * @name ChatDots light version
 */

const ChatDotsLight = () => {
  const animation = useRef(null)

  useEffect(() => {
    if (animation.current) {
      lottie.loadAnimation({
        name: 'dotsAnimation',
        container: animation.current,
        renderer: 'svg',
        loop: true,
        autoplay: true,
        animationData: dotsChatLottieLight,
      })
    }

    return () => {
      try {
        lottie.destroy()
      } catch (_) {
        // Do not propagate it further
      }
    }
  }, [animation])

  return (
    <StyledRow>
      <DotsContainer ref={animation} />
    </StyledRow>
  )
}

/**
 * @name ChatDots dark version
 */

const ChatDotsDark = () => {
  const animation = useRef(null)
  useEffect(() => {
    if (animation.current) {
      lottie.loadAnimation({
        name: 'dotsAnimation',
        container: animation.current,
        renderer: 'svg',
        loop: true,
        autoplay: true,
        animationData: dotsChatLottieDark,
      })
    }

    return () => {
      try {
        lottie.destroy()
      } catch (_) {
        // Do not propagate it further
      }
    }
  }, [animation])

  return (
    <StyledRow>
      <DotsContainer ref={animation} />
    </StyledRow>
  )
}

export { ChatDotsLight, ChatDotsDark }
