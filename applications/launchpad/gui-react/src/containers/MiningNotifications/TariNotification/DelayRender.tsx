import { useState, useEffect } from 'react'

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

    return () => clearTimeout(timeout)
  }, [render])

  return renderAlready ? render() : null
}

export default DelayRender
