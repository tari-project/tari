import { render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../styles/themes'
import SettingsSectionHeader from './'

describe('SettingsSectionHeader', () => {
  it('should render text when is given', async () => {
    const testText = 'Expert view'
    render(
      <ThemeProvider theme={themes.light}>
        <SettingsSectionHeader>{testText}</SettingsSectionHeader>
      </ThemeProvider>,
    )

    const label = screen.getByText(testText)
    expect(label).toBeInTheDocument()
  })

  it('should render without crash when children is not set', async () => {
    render(
      <ThemeProvider theme={themes.light}>
        <SettingsSectionHeader />
      </ThemeProvider>,
    )

    const label = screen.getByTestId('settings-section-header-cmp')
    expect(label).toBeInTheDocument()
  })
})
