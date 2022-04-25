import styles from '../styles'

const lightTheme = {
  primary: styles.colors.dark.primary,
  secondary: styles.colors.dark.secondary,
  tertiary: styles.colors.dark.tertiary,
  background: styles.colors.light.primary,
  backgroundSecondary: styles.colors.light.background,
  backgroundImage: styles.colors.light.backgroundImage,
  accent: styles.colors.tari.purple,
  accentDark: styles.colors.tari.purpleDark,
  disabledText: styles.colors.dark.placeholder,
  tariGradient: styles.gradients.tari,
  borderColor: styles.colors.dark.borders,
  borderColorLight: styles.colors.secondary.borderLight,
  actionBackground: styles.colors.secondary.actionBackground,
  controlBackground: 'transparent',

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
    borderColor: styles.colors.secondary.borderLight,
    borderColorLight: styles.colors.secondary.borderLight,
    actionBackground: styles.colors.secondary.actionBackground,
    controlBackground: 'rgba(255,255,255,.2)',
  },
}

export default lightTheme
