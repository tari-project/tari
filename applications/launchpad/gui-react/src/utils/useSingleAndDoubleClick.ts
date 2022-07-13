import { useState, useEffect } from 'react'

const useSingleAndDoubleClick = (
  { single, double }: { single: () => void; double: () => void },
  delay = 300,
) => {
  const [click, setClick] = useState(0)

  useEffect(() => {
    const timer = setTimeout(() => {
      setClick(0)
    }, delay)

    if (click === 2) {
      double()
    }

    return () => clearTimeout(timer)
  }, [click])

  return () => {
    single()
    setClick(prev => prev + 1)
  }
}

export default useSingleAndDoubleClick
