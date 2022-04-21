import { DefaultTheme } from 'styled-components'

import darkTheme from './dark'
import lightTheme from './light'

const SPACING = 24

const withShared = (theme: DefaultTheme): DefaultTheme => ({
  ...theme,
  borderRadius: (count = 1) => `${count * 12}px`,
  spacing: (count = 1) => `${count * SPACING}px`,
  spacingVertical: (count = 1) => `${count * (0.54 * SPACING)}px`,
  spacingHorizontal: (count = 1) => `${count * SPACING}px`,
})

const themes = {
  light: withShared(lightTheme),
  dark: withShared(darkTheme),
}

export default themes
