import darkTheme from './dark'
import lightTheme from './light'

const withShared = theme => ({
  ...theme,
  borderRadius: (count = 1) => `${count * 10}px`,
  spacingVertical: (count = 1) => `${count * 0.7}em`,
  spacingHorizontal: (count = 1) => `${count * 1.3}em`,
})

const themes = {
  light: withShared(lightTheme),
  dark: withShared(darkTheme),
}

export default themes
