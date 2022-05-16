import { fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import { Schedule } from '../../../../types/general'
import themes from '../../../../styles/themes'

import ScheduleForm from './'

describe('ScheduleForm', () => {
  it('should not activate save button until mining type and schedule is selected', () => {
    const onChange = jest.fn()
    const schedule = {
      id: 'scheduleId',
      interval: {
        from: { hours: 8, minutes: 0 },
        to: { hours: 9, minutes: 0 },
      },
    } as unknown as Schedule
    render(
      <ThemeProvider theme={themes.light}>
        <ScheduleForm
          value={schedule}
          cancel={() => null}
          remove={() => null}
          onChange={onChange}
        />
      </ThemeProvider>,
    )

    expect(screen.getByText('Save').closest('button')).toBeDisabled()

    fireEvent.click(screen.getByText('Tari Mining'))
    expect(screen.getByText('Save').closest('button')).toBeDisabled()

    fireEvent.click(screen.getByText('M'))
    expect(screen.getByText('Save').closest('button')).not.toBeDisabled()

    fireEvent.click(screen.getByText('Save'))
    expect(onChange).toHaveBeenCalledTimes(1)
  })

  it('should not render remove schedule button if no initial schedule values were passed', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <ScheduleForm
          cancel={() => null}
          remove={() => null}
          onChange={() => null}
        />
      </ThemeProvider>,
    )

    expect(screen.queryByText('Remove schedule')).not.toBeInTheDocument()
  })

  it('should allow removing schedule when it is passed to edit', () => {
    const remove = jest.fn()
    const schedule = { id: 'scheduleId' } as unknown as Schedule
    render(
      <ThemeProvider theme={themes.light}>
        <ScheduleForm
          value={schedule}
          cancel={() => null}
          remove={remove}
          onChange={() => null}
        />
      </ThemeProvider>,
    )

    const removeButton = screen.getByText('Remove schedule')
    expect(removeButton).toBeInTheDocument()

    fireEvent.click(removeButton)
    expect(remove).toHaveBeenCalledTimes(1)
  })

  it('attempting to save schedule with invalid interval should raise error', () => {
    const schedule = {
      id: 'scheduleId',
      type: ['tari'],
      days: [2],
      interval: {
        from: { hours: 8, minutes: 0 },
        to: { hours: 8, minutes: 0 },
      },
    } as unknown as Schedule
    render(
      <ThemeProvider theme={themes.light}>
        <ScheduleForm
          value={schedule}
          cancel={() => null}
          remove={() => null}
          onChange={() => null}
        />
      </ThemeProvider>,
    )

    expect(screen.queryByText('Ops!')).not.toBeInTheDocument()

    const saveButton = screen.getByText('Save')
    fireEvent.click(saveButton)

    expect(screen.queryByText('Ops!')).toBeInTheDocument()
    expect(screen.queryByText('Try again')).toBeInTheDocument()
  })

  it('cancelling on error should get you back to list', () => {
    const cancel = jest.fn()
    const schedule = {
      id: 'scheduleId',
      type: ['tari'],
      days: [2],
      interval: {
        from: { hours: 8, minutes: 0 },
        to: { hours: 8, minutes: 0 },
      },
    } as unknown as Schedule
    render(
      <ThemeProvider theme={themes.light}>
        <ScheduleForm
          value={schedule}
          cancel={cancel}
          remove={() => null}
          onChange={() => null}
        />
      </ThemeProvider>,
    )

    const saveButton = screen.getByText('Save')
    fireEvent.click(saveButton)

    const errorBox = screen
      .queryByText('Try again')
      ?.closest('[data-testid="box-cmp"]')
    const buttons = Array.from(errorBox?.querySelectorAll('button') || [])
    const cancelButton = buttons?.find(
      (b: HTMLElement) => b.textContent === 'Cancel',
    )
    expect(cancelButton).toBeInTheDocument()

    fireEvent.click(cancelButton as HTMLElement)
    expect(cancel).toHaveBeenCalledTimes(1)
  })
})
