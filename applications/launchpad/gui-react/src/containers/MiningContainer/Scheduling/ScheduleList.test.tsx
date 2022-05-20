import { fireEvent, render, screen } from '@testing-library/react'
import { ThemeProvider } from 'styled-components'

import themes from '../../../styles/themes'
import { MiningNodeType } from '../../../types/general'
import t from '../../../locales'

import ScheduleList from './ScheduleList'

const exampleSchedules = [
  {
    id: 'example-schedule',
    testId: 'schedule-example-schedule',
    enabled: true,
    days: [0, 1, 2],
    interval: {
      from: { hours: 3, minutes: 0 },
      to: { hours: 19, minutes: 35 },
    },
    type: ['merged'] as MiningNodeType[],
  },
  {
    id: 'example-schedule-2',
    testId: 'schedule-example-schedule-2',
    enabled: true,
    days: [0, 1, 2],
    interval: {
      from: { hours: 3, minutes: 0 },
      to: { hours: 19, minutes: 35 },
    },
    type: ['merged'] as MiningNodeType[],
  },
]

describe('ScheduleList', () => {
  it('should render Add schedule button when list of schedules is empty', () => {
    const addSchedule = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <ScheduleList
          schedules={[]}
          cancel={() => null}
          addSchedule={addSchedule}
          toggle={() => null}
          edit={() => null}
          remove={() => null}
        />
      </ThemeProvider>,
    )

    const addScheduleButton = screen.getByText(t.mining.scheduling.add)
    expect(addScheduleButton).toBeInTheDocument()

    fireEvent.click(addScheduleButton)
    expect(addSchedule).toHaveBeenCalledTimes(1)
  })

  it('should still render Add schedule button when non empty schedule list is provided', () => {
    const addSchedule = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <ScheduleList
          schedules={exampleSchedules}
          cancel={() => null}
          addSchedule={addSchedule}
          toggle={() => null}
          edit={() => null}
          remove={() => null}
        />
      </ThemeProvider>,
    )

    const addScheduleButton = screen.getByText(t.mining.scheduling.add)
    expect(addScheduleButton).toBeInTheDocument()

    fireEvent.click(addScheduleButton)
    expect(addSchedule).toHaveBeenCalledTimes(1)
  })

  it('should render all schedules', () => {
    render(
      <ThemeProvider theme={themes.light}>
        <ScheduleList
          schedules={exampleSchedules}
          cancel={() => null}
          addSchedule={() => null}
          toggle={() => null}
          edit={() => null}
          remove={() => null}
        />
      </ThemeProvider>,
    )

    expect(screen.getByTestId(exampleSchedules[0].testId)).toBeInTheDocument()
    expect(screen.getByTestId(exampleSchedules[1].testId)).toBeInTheDocument()
  })

  it('should select schedule on single click', () => {
    const { container } = render(
      <ThemeProvider theme={themes.light}>
        <ScheduleList
          schedules={exampleSchedules}
          cancel={() => null}
          addSchedule={() => null}
          toggle={() => null}
          edit={() => null}
          remove={() => null}
        />
      </ThemeProvider>,
    )

    const schedule = screen.getByTestId(exampleSchedules[0].testId)
    fireEvent.click(schedule)

    const selectedSchedule = container.querySelector('[data-selected="true"]')
    expect(selectedSchedule).toBeInTheDocument()
  })

  it('should both select and call for edit on double click ', () => {
    const edit = jest.fn()
    const { container } = render(
      <ThemeProvider theme={themes.light}>
        <ScheduleList
          schedules={exampleSchedules}
          cancel={() => null}
          addSchedule={() => null}
          toggle={() => null}
          edit={edit}
          remove={() => null}
        />
      </ThemeProvider>,
    )

    const schedule = screen.getByTestId(exampleSchedules[0].testId)
    fireEvent.click(schedule)
    fireEvent.click(schedule)

    const selectedSchedule = container.querySelector('[data-selected="true"]')
    expect(selectedSchedule).toBeInTheDocument()

    expect(edit).toHaveBeenCalledTimes(1)
    expect(edit).toHaveBeenCalledWith(exampleSchedules[0].id)
  })

  it('should toggle schedule on switch click', () => {
    const toggle = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <ScheduleList
          schedules={exampleSchedules}
          cancel={() => null}
          addSchedule={() => null}
          toggle={toggle}
          edit={() => null}
          remove={() => null}
        />
      </ThemeProvider>,
    )

    const switchComponent = screen.getAllByTestId('switch-input-cmp')[0]
    fireEvent.click(switchComponent)

    expect(toggle).toHaveBeenCalledTimes(1)
    expect(toggle).toHaveBeenCalledWith(exampleSchedules[0].id)
  })

  it('should remove schedule on delete or backspace', () => {
    const remove = jest.fn()
    render(
      <ThemeProvider theme={themes.light}>
        <ScheduleList
          schedules={exampleSchedules}
          cancel={() => null}
          addSchedule={() => null}
          toggle={() => null}
          edit={() => null}
          remove={remove}
        />
      </ThemeProvider>,
    )

    const schedule1 = screen.getByTestId(exampleSchedules[0].testId)
    fireEvent.click(schedule1)
    fireEvent.keyDown(schedule1, { key: 'Delete' })

    const schedule2 = screen.getByTestId(exampleSchedules[1].testId)
    fireEvent.click(schedule2)
    fireEvent.keyDown(schedule2, { key: 'Backspace' })

    expect(remove).toHaveBeenCalledTimes(2)
    expect(remove).toHaveBeenCalledWith(exampleSchedules[0].id)
    expect(remove).toHaveBeenCalledWith(exampleSchedules[1].id)
  })
})
