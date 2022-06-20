import { useState, useEffect } from 'react'

/**
 * @name DelayRender
 * @description component that renders elements with delay
 *
 * @prop {() => JSX.Element} render - render prop
 * @prop {number} [delay] - optional delay in ms (400ms by default)
 */
const DelayRender = ({
  delay = 400,
  render,
}: {
  delay?: number
  render: () => JSX.Element
}) => {
  const [renderAlready, setRenderAlready] = useState(false)

  useEffect(() => {
    setRenderAlready(false)
    const timeout = setTimeout(() => setRenderAlready(true), delay)

    return () => {
      setRenderAlready(false)
      clearTimeout(timeout)
    }
  }, [render])

  return renderAlready ? render() : null
}

export default DelayRender
