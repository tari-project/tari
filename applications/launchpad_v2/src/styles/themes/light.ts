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
  borderColor: styles.colors.dark.borders,
  actionBackground: styles.colors.secondary.actionBackground,
  resetBackground: styles.colors.light.overlay,
  resetBackgroundDark: styles.colors.light.overlayDark,

  titleBar: styles.colors.light.background,

  controlBackground: 'transparent',

  inverted: {
    controlBackground: 'rgba(255,255,255,.2)',
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
