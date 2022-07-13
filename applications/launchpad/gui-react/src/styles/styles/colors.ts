const colors = {
  dark: {
    primary: '#20053D',
    secondary: '#716A78',
    tertiary: '#837A8B',
    borders: '#EDECEE',
    placeholder: '#D6D4D9',
    input: '#000000',
  },
  light: {
    primary: '#FFFFFF',
    textSecondary: '#FFFFFFa3',
    overlay: '#FFFFFF38',
    overlayDark: '#FFFFFF70',
    backgroundImage: '#F6F5F8',
    background: '#FAFAFA',
    gray: '#E5E5E5',
    graySecondary: '#EEECF1',
  },
  tari: {
    purpleDark: '#662FA1',
    purple: '#9330FF',
  },
  monero: {
    dark: '#A2281D',
  },
  merged: {
    dark: '#4A125854',
  },
  secondary: {
    onText: '#094E41',
    onTextLight: '#5F9C91',
    on: '#E6FAF6',
    info: '#ECF0FE',
    infoText: '#4D6FE8',
    warning: '#FFEED3',
    warningText: '#D18A18',
    warningDark: '#D85240',
    error: '#E7362A',
    actionBackground: '#76A59D',
    borderLight: '#DBDBDD',
    greenMedium: '#5F9C91',
    tbotBubble: 'rgba(32, 5, 61, 0.02)',
  },
  darkMode: {
    modalBackground: '#141414',
    modalBackgroundSecondary: '#000000',
    borders: '#222222',
    dashboard: '#0A0A0A',
    input: '#000000',
    tags: '#00000040',
    logoCard: '#00000030',
    darkLogoCard: '#00000060',
    disabledText: '#47434A',
    baseNodeStart: '#455E5B',
    baseNodeEnd: '#55208E',
    message: '#262626',
  },
  graph: {
    fuchsia: '#EF5DA8',
    yellow: '#EEDC3C',
    lightGreen: '#78E590',
  },
}

export const chartColors = [
  colors.secondary.infoText,
  colors.secondary.onTextLight,
  colors.secondary.warningDark,
  colors.graph.fuchsia,
  colors.secondary.warning,
  colors.tari.purple,
  colors.graph.yellow,
  colors.graph.lightGreen,
]

export default colors
