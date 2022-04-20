import { useTheme } from './useTheme'

export const WithTheme = (Component) => function TT(props) {
  const theme = useTheme()

  return <Component {...props} theme={theme} />
}

