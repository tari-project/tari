import styles from '../styles'

const lightTheme = {
  primary: styles.colors.dark.primary,
  secondary: styles.colors.dark.secondary,
  background: styles.colors.light.primary,
  backgroundSecondary: styles.colors.light.background,
  backgroundImage: styles.colors.light.backgroundImage,
  accent: styles.colors.tari.purple,
  accentDark: styles.colors.tari.purpleDark,
  disabledText: styles.colors.dark.placeholder,
  tariGradient: styles.gradients.tari,

  titleBar: styles.colors.light.background,

  borderColor: styles.colors.dark.borders,
  borderRadius: (count = 1) => `${count * 10}px`,
  selected: styles.colors.light.backgroundImage,
  spacingVertical: (count = 1) => `${count * 0.7}em`,
  spacingHorizontal: (count = 1) => `${count * 1.3}em`,
  transparentBackground: 'rgba(255,255,255,.2)',

  inverted: {
    primary: styles.colors.light.primary,
    secondary: styles.colors.dark.secondary,
    background: styles.colors.light.primary,
    backgroundSecondary: styles.colors.darkMode.modalBackground,
    backgroundImage: styles.colors.light.backgroundImage,
    accent: styles.colors.tari.purple,
    accentDark: styles.colors.tari.purpleDark,
    disabledText: styles.colors.dark.placeholder,
    tariGradient: styles.gradients.tari,
  },
}

export default lightTheme
