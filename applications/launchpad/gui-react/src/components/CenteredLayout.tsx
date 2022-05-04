import styled from 'styled-components'

const CenteredLayout = styled.div<{
  horizontally?: boolean
  vertically?: boolean
}>`
  display: flex;
  flex-wrap: wrap;
  justify-content: ${({ horizontally }) => (horizontally ? 'center' : 'left')};
  align-items: ${({ vertically }) => (vertically ? 'center' : 'flex-start')};
  min-height: ${({ vertically }) => (vertically ? '100%' : '0')};
  column-gap: ${({ theme }) => theme.spacing()};
  position: relative;
`
export default CenteredLayout
