import { useSelector } from 'react-redux'

import { selectThemeConfig } from '../store/app/selectors'

export const useTheme = () => useSelector(selectThemeConfig)
