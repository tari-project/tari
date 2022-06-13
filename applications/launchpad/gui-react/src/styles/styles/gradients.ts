import colors from './colors'

/**
 * Gradient palette
 */

const gradients = {
  tari: `linear-gradient(239.91deg, ${colors.tari.purple} 0%, #593A9B 131%)`,
  tariText: `background: -webkit-linear-gradient(${colors.tari.purple}, #593A9B); -webkit-background-clip: text; -webkit-text-fill-color: transparent;`,
  tariDark: 'linear-gradient(239.91deg, #3D1061 0%, #593A9B 131%)',
  monero: 'linear-gradient(239.91deg, #ED695E 0%, #D24F43 131%)',
  merged: 'linear-gradient(238.06deg, #6838B4 0%, #DA574B 99.74%)',
  mergedDark: 'linear-gradient(238.06deg, #3F1264 0%, #782D27 99.74%)',
  baseNodeDark: 'linear-gradient(238.06deg, #55208E 0%, #455E5B 99.74%)',
  warning: `linear-gradient(264.94deg, #D87740 21.86%, ${colors.secondary.warningText} 80.58%)`,
}

export default gradients
