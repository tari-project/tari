import styled, { useTheme } from 'styled-components'

import Box from '../../../components/Box'

export const ScheduleContainer = styled.div`
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  align-items: center;
  height: 100%;
`

export const NoSchedulesContainer = styled.div`
  flex-grow: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
`

export const SchedulesListContainer = styled.div`
  flex-grow: 1;
  display: flex;
  flex-direction: column;
  justify-content: flex-start;
  align-items: center;
`

export const Actions = (props: any) => {
  const theme = useTheme()

  return (
    <Box
      {...props}
      border={false}
      style={{
        width: '100%',
        borderTopLeftRadius: 0,
        borderTopRightRadius: 0,
        borderTop: `1px solid ${theme.borderColor}`,
        marginBottom: 0,
        display: 'flex',
        justifyContent: 'flex-end',
      }}
    />
  )
}
