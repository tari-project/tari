import { DefaultTheme } from 'styled-components'

import darkTheme from './dark'
import lightTheme from './light'

const SPACING = 24

const withShared = (theme: Record<string, unknown>): DefaultTheme =>
  ({
    ...theme,
    borderRadius: (count = 1) => `${count * 12}px`,
    tightBorderRadius: (count = 1) => `${count * 8}px`,
    spacing: (count = 1) => `${count * SPACING}px`,
    spacingVertical: (count = 1) => `${count * (0.54 * SPACING)}px`,
    spacingHorizontal: (count = 1) => `${count * SPACING}px`,
    tabsMarginRight: 32,
  } as DefaultTheme)

const themes = {
  light: withShared(lightTheme),
  dark: withShared(darkTheme),
}

export default themes
