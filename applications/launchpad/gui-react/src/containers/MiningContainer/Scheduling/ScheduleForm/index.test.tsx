import { fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import { Schedule, MiningNodeType } from '../../../../types/general'
import themes from '../../../../styles/themes'
import t from '../../../../locales'

import ScheduleForm from './'

const ALL_MINING_ACTIVE = ['tari', 'merged'] as MiningNodeType[]

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
          miningTypesActive={ALL_MINING_ACTIVE}
        />
      </ThemeProvider>,
    )

    expect(
      screen.getByText(t.common.verbs.save).closest('button'),
    ).toBeDisabled()

    fireEvent.click(screen.getByText(t.common.miningType.tari))
    expect(
      screen.getByText(t.common.verbs.save).closest('button'),
    ).toBeDisabled()

    fireEvent.click(screen.getByText(t.common.weekdayCapitals.monday))
    expect(
      screen.getByText(t.common.verbs.save).closest('button'),
    ).not.toBeDisabled()

    fireEvent.click(screen.getByText(t.common.verbs.save))
    expect(onChange).toHaveBeenCalledTimes(1)
  })

  it('should not render remove schedule button if no initial schedule values were passed', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <ScheduleForm
          cancel={() => null}
          remove={() => null}
          onChange={() => null}
          miningTypesActive={ALL_MINING_ACTIVE}
        />
      </ThemeProvider>,
    )

    expect(
      screen.queryByText(t.mining.scheduling.removeSchedule),
    ).not.toBeInTheDocument()
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
          miningTypesActive={ALL_MINING_ACTIVE}
        />
      </ThemeProvider>,
    )

    const removeButton = screen.getByText(t.mining.scheduling.removeSchedule)
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
          miningTypesActive={ALL_MINING_ACTIVE}
        />
      </ThemeProvider>,
    )

    expect(screen.queryByText(t.mining.scheduling.ops)).not.toBeInTheDocument()

    const saveButton = screen.getByText(t.common.verbs.save)
    fireEvent.click(saveButton)

    expect(screen.queryByText(t.mining.scheduling.ops)).toBeInTheDocument()
    expect(screen.queryByText(t.common.verbs.tryAgain)).toBeInTheDocument()
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
          miningTypesActive={ALL_MINING_ACTIVE}
        />
      </ThemeProvider>,
    )

    const saveButton = screen.getByText(t.common.verbs.save)
    fireEvent.click(saveButton)

    const errorBox = screen
      .queryByText(t.common.verbs.tryAgain)
      ?.closest('[data-testid="box-cmp"]')
    const buttons = Array.from(errorBox?.querySelectorAll('button') || [])
    const cancelButton = buttons?.find(
      (b: HTMLElement) => b.textContent === t.common.verbs.cancel,
    )
    expect(cancelButton).toBeInTheDocument()

    fireEvent.click(cancelButton as HTMLElement)
    expect(cancel).toHaveBeenCalledTimes(1)
  })
})
