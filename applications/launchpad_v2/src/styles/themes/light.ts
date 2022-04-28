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
  resetBackground: styles.colors.light.overlay,
  shadow: '0 0 40px #00000011',

  titleBar: styles.colors.light.background,

  controlBackground: 'transparent',
  info: styles.colors.secondary.info,
  infoText: styles.colors.secondary.infoText,
  on: styles.colors.secondary.on,
  onText: styles.colors.secondary.onText,
  onTextLight: styles.colors.secondary.onTextLight,
  warning: styles.colors.secondary.warning,
  warningText: styles.colors.secondary.warningText,
  expert: 'rgba(147, 48, 255, 0.05)',
  expertText: styles.gradients.tari,
  placeholderText: styles.colors.dark.placeholder,

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
