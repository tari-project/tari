import { SVGProps } from 'react'

const SvgTariSignet = (props: SVGProps<SVGSVGElement>) => (
  <svg
    data-testid='svg-tarisignet'
    xmlns='http://www.w3.org/2000/svg'
    width='28'
    height='28'
    viewBox='0 0 28 28'
    fill='none'
    {...props}
  >
    <path
      d='M0 7.46289V15.259L11.238 27.9974L27.1782 15.3038V7.45476L11.2954 0.00146484L0 7.46289ZM9.88868 22.3261L2.72276 14.1952V9.77748L9.88868 11.6489V22.3261ZM12.6168 23.3913V12.3658L23.0367 15.0842L12.6168 23.3913ZM24.4581 9.22322V12.603L4.92899 7.50354L11.5143 3.15895L24.4581 9.22322Z'
      fill='currentColor'
    />
  </svg>
)

export default SvgTariSignet
