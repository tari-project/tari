import styles from '../styles'

const lightTheme = {
  primary: styles.colors.dark.primary,
  secondary: styles.colors.dark.secondary,
  background: styles.colors.light.primary,
  backgroundImage: styles.colors.light.backgroundImage,
  accent: styles.colors.tari.purple,
  accentDark: styles.colors.tari.purpleDark,
  disabledText: styles.colors.dark.placeholder,
  tariGradient: styles.gradients.tari,

  titleBar: styles.colors.light.background,

  borderColor: styles.colors.dark.borders,
  selected: styles.colors.light.backgroundImage,
  borderRadius: (count = 1) => `${count * 10}px`,
  spacingVertical: (count = 1) => `${count * 0.7}em`,
  spacingHorizontal: (count = 1) => `${count * 1.3}em`,
}

export default lightTheme
