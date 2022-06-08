import styled from 'styled-components'

export const ChartContainer = styled.div`
  background-color: ${({ theme }) => theme.inverted.backgroundSecondary};
  padding: ${({ theme }) => theme.spacing()};
  border-radius: ${({ theme }) => theme.borderRadius()};
  max-width: 100%;
`

export const Legend = styled.div`
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  column-gap: ${({ theme }) => theme.spacing()};
`

export const LegendItem = styled.div`
  display: flex;
  align-items: center;
  column-gap: ${({ theme }) => theme.spacing(0.5)};
  min-height: 1em;
`

export const SeriesColorIndicator = styled.div<{ color: string }>`
  width: 1em;
  height: 0.1em;
  border-radius: 2px;
  background-color: ${({ color }) => color};
`
