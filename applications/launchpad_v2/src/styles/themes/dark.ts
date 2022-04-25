import styles from '../styles'

const darkTheme = {
  primary: styles.colors.light.primary,
  secondary: styles.colors.dark.secondary,
  tertiary: styles.colors.dark.tertiary,
  background: styles.colors.darkMode.modalBackgroundSecondary,
  backgroundSecondary: styles.colors.darkMode.modalBackground,
  backgroundImage: styles.colors.light.backgroundImage,
  accent: styles.colors.tari.purple,
  accentDark: styles.colors.tari.purpleDark,
  disabledText: styles.colors.dark.placeholder,
  tariGradient: styles.gradients.tari,
  borderColor: styles.colors.light.backgroundImage,
  borderColorLight: styles.colors.secondary.borderLight,
  controlBackground: 'rgba(255,255,255,.2)',

  inverted: {
    primary: styles.colors.light.primary,
    secondary: styles.colors.dark.secondary,
    tertiary: styles.colors.dark.tertiary,
    background: styles.colors.darkMode.modalBackgroundSecondary,
    backgroundSecondary: styles.colors.darkMode.modalBackground,
    backgroundImage: styles.colors.light.backgroundImage,
    accent: styles.colors.tari.purple,
    accentDark: styles.colors.tari.purpleDark,
    disabledText: styles.colors.dark.placeholder,
    tariGradient: styles.gradients.tari,
    borderColor: styles.colors.light.backgroundImage,
    borderColorLight: styles.colors.secondary.borderLight,
    controlBackground: 'transparent',
  },
}

export default darkTheme
