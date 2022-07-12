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
  accentMonero: styles.colors.secondary.warningDark,
  accentMerged: styles.colors.merged.dark,
  disabledText: styles.colors.dark.placeholder,
  tariGradient: styles.gradients.tari,
  tariTextGradient: styles.gradients.tariText,
  mergedGradient: styles.gradients.merged,
  warningGradient: styles.gradients.warning,
  greenMedium: styles.colors.secondary.greenMedium,
  borderColor: styles.colors.dark.borders,
  borderColorLight: styles.colors.secondary.borderLight,
  actionBackground: styles.colors.secondary.actionBackground,
  resetBackground: styles.colors.light.overlay,
  resetBackgroundHover: styles.colors.light.overlayDark,
  moneroDark: styles.colors.monero.dark,
  shadow40: '0 0 40px #00000011',
  shadow24: '0 0 24px #00000006',

  titleBar: styles.colors.light.background,
  controlBackground: 'transparent',
  infoTag: styles.colors.secondary.info,
  infoText: styles.colors.secondary.infoText,
  on: styles.colors.secondary.on,
  onText: styles.colors.secondary.onText,
  onTextLight: styles.colors.secondary.onTextLight,
  warning: styles.colors.secondary.warning,
  warningTag: styles.colors.secondary.warning,
  warningText: styles.colors.secondary.warningText,
  warningDark: styles.colors.secondary.warningDark,
  success: styles.colors.secondary.onTextLight,
  error: styles.colors.secondary.error,
  expert: 'rgba(147, 48, 255, 0.05)',
  expertText: styles.gradients.tari,
  expertSecText: styles.colors.tari.purple,
  lightTag: styles.colors.light.backgroundImage,
  lightTagText: styles.colors.dark.secondary,
  placeholderText: styles.colors.dark.placeholder,
  textSecondary: styles.colors.light.textSecondary,
  helpTipText: styles.colors.dark.primary,
  runningTagBackground: styles.colors.secondary.on,
  runningTagText: styles.colors.secondary.onText,
  dashboardRunningTagText: styles.colors.secondary.onText,
  dashboardRunningTagBackground: styles.colors.secondary.on,
  switchBorder: styles.colors.dark.primary,
  switchCircle: styles.colors.light.background,
  switchController: styles.colors.light.background,
  nodeBackground: styles.colors.light.primary,
  nodeLightIcon: styles.colors.light.backgroundImage,
  nodeSubHeading: styles.colors.dark.primary,
  nodeWarningText: styles.colors.dark.secondary,
  calloutBackground: styles.colors.secondary.warning,
  inputPlaceholder: styles.colors.dark.placeholder,
  disabledPrimaryButton: styles.colors.light.backgroundImage,
  disabledPrimaryButtonText: styles.colors.dark.placeholder,
  baseNodeGradientStart: styles.colors.secondary.actionBackground,
  baseNodeGradientEnd: styles.colors.tari.purple,
  baseNodeRunningLabel: styles.colors.light.primary,
  baseNodeRunningTagBackground: styles.colors.secondary.on,
  baseNodeRunningTagText: styles.colors.secondary.onText,
  selectBorderColor: styles.colors.dark.borders,
  selectOptionHover: styles.colors.light.backgroundImage,
  walletSetupBorderColor: 'transparent',
  walletCopyBoxBorder: styles.colors.dark.borders,
  balanceBoxBorder: styles.colors.light.backgroundImage,
  walletBottomBox: styles.colors.light.background,
  modalBackdrop: styles.colors.light.primary,
  settingsMenuItem: styles.colors.tari.purpleDark,
  settingsMenuItemActive: styles.colors.light.backgroundImage,
  settingsCopyBoxBackground: styles.colors.light.backgroundImage,
  scrollBarTrack: styles.colors.light.background,
  scrollBarThumb: styles.colors.darkMode.tags,
  scrollBarHover: styles.colors.darkMode.darkLogoCard,
  tbotMessage: styles.colors.light.primary,

  inverted: {
    primary: styles.colors.light.primary,
    secondary: styles.colors.light.textSecondary,
    tertiary: styles.colors.dark.tertiary,
    background: styles.colors.darkMode.modalBackgroundSecondary,
    backgroundSecondary: styles.colors.darkMode.modalBackground,
    backgroundImage: styles.colors.light.backgroundImage,
    accent: styles.colors.tari.purple,
    accentSecondary: styles.colors.secondary.onTextLight,
    accentDark: styles.colors.tari.purpleDark,
    accentMonero: styles.colors.secondary.warningDark,
    accentMerged: styles.colors.merged.dark,
    disabledText: styles.colors.dark.placeholder,
    tariGradient: styles.gradients.tari,
    tariTextGradient: styles.gradients.tariText,
    mergedGradient: styles.gradients.merged,
    warningGradient: styles.gradients.warning,
    greenMedium: styles.colors.secondary.greenMedium,
    infoTag: styles.colors.secondary.info,
    infoText: styles.colors.secondary.infoText,
    on: styles.colors.secondary.on,
    onText: styles.colors.secondary.onText,
    onTextLight: styles.colors.secondary.onTextLight,
    warning: styles.colors.secondary.warning,
    warningText: styles.colors.secondary.warningText,
    warningDark: styles.colors.secondary.warningDark,
    success: styles.colors.secondary.onTextLight,
    error: styles.colors.secondary.error,
    expert: 'rgba(147, 48, 255, 0.05)',
    expertText: styles.gradients.tari,
    expertSecText: styles.colors.tari.purple,
    lightTag: styles.colors.light.backgroundImage,
    lightTagText: styles.colors.dark.secondary,
    borderColor: styles.colors.secondary.borderLight,
    borderColorLight: styles.colors.secondary.borderLight,
    actionBackground: styles.colors.secondary.actionBackground,
    resetBackground: styles.colors.light.overlay,
    resetBackgroundHover: styles.colors.light.overlayDark,
    moneroDark: styles.colors.monero.dark,
    controlBackground: 'rgba(255,255,255,.2)',
    helpTipText: styles.colors.dark.primary,
    runningTagBackground: styles.colors.secondary.on,
    runningTagText: styles.colors.secondary.onText,
    dashboardRunningTagText: styles.colors.secondary.onText,
    dashboardRunningTagBackground: styles.colors.secondary.on,
    switchBorder: styles.colors.dark.primary,
    switchController: styles.colors.light.background,
    nodeBackground: styles.colors.light.primary,
    nodeLightIcon: styles.colors.light.backgroundImage,
    nodeSubHeading: styles.colors.dark.primary,
    nodeWarningText: styles.colors.dark.secondary,
    calloutBackground: styles.colors.secondary.warning,
    inputPlaceholder: styles.colors.dark.placeholder,
    disabledPrimaryButton: styles.colors.light.backgroundImage,
    disabledPrimaryButtonText: styles.colors.dark.placeholder,
    baseNodeGradientStart: styles.colors.secondary.actionBackground,
    baseNodeGradientEnd: styles.colors.tari.purple,
    baseNodeRunningLabel: styles.colors.light.primary,
    baseNodeRunningTagBackground: styles.colors.secondary.on,
    baseNodeRunningTagText: styles.colors.secondary.onText,
    selectBorderColor: styles.colors.dark.borders,
    selectOptionHover: styles.colors.light.backgroundImage,
    walletSetupBorderColor: 'transparent',
    walletCopyBoxBorder: styles.colors.dark.borders,
    balanceBoxBorder: styles.colors.light.backgroundImage,
    walletBottomBox: styles.colors.light.background,
    modalBackdrop: styles.colors.light.primary,
    settingsMenuItem: styles.colors.tari.purpleDark,
    settingsMenuItemActive: styles.colors.light.backgroundImage,
    settingsCopyBoxBackground: styles.colors.light.backgroundImage,
    scrollBarTrack: styles.colors.light.textSecondary,
    tbotMessage: styles.colors.light.primary,
  },
}

export default lightTheme
