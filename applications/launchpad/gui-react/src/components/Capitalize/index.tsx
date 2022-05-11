import styled from 'styled-components'

const CapitalizeSpan = styled.span`
  text-transform: capitalize;
`

const Capitalize = ({ children }: { children: string }) => (
  <CapitalizeSpan>{children}</CapitalizeSpan>
)

export default Capitalize
