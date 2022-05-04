import { useSpring } from 'react-spring'
import { ExpertViewType } from '../store/app/types'

/**
 * Helpers for the ExpertView
 */
const ExpertViewUtils = {
  /**
   * Helper used to convert 'hidden', 'open', 'fullscreen' into actual '%' value
   * @param {ExpertViewType} expertView
   * @param {string} [openViewSize = '50%'] - the value for 'open' mode, ie. '40%'
   */
  convertExpertViewModeToValue: (
    expertView: ExpertViewType,
    openViewSize = '50%',
  ) => {
    let size = '0%'
    switch (expertView) {
      case 'open':
        size = openViewSize
        break
      case 'fullscreen':
        size = '100%'
        break
      default:
        size = '0%'
        break
    }

    const invertedSize = `${100 - parseFloat(size)}%`
    return [size, invertedSize]
  },

  /**
   * Animate the Expert View drawer
   * @param size
   */
  useDrawerAnim: (size: string) => useSpring({ width: size }),
}

export default ExpertViewUtils
