import { useEffect, useRef } from 'react'

/**
 * Hook using `requestAnimationFrame` to smoothly run given callback.
 * @param {(val: number) => void} callback
 *
 * @example
 * const [counter, setCounter] = useState(0)
 *
 * useAnimationFrame(() => {
 *   setCounter((t) => {
 *     return t + 1
 *   })
 * })
 *
 * console.log(counter)
 */
const useAnimationFrame = (callback: (val: number) => void, active = true) => {
  const requestRef = useRef<number>()

  const animate = (val: number) => {
    callback(val)
    requestRef.current = requestAnimationFrame(animate)
  }

  useEffect(() => {
    if (active) {
      requestRef.current = requestAnimationFrame(animate)
    }
    return () => {
      if (requestRef.current) {
        cancelAnimationFrame(requestRef.current)
      }
    }
  }, [active])

  return requestRef
}

export default useAnimationFrame
