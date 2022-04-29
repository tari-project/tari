import { SVGProps } from 'react'

const SvgSmileyNot = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0.5 0.5 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-smileynot'
    {...props}
  >
    <path
      d='M6.75 19.875a7.502 7.502 0 0 0 2.575 1.147c2.006.47 4.094.47 6.1 0a7.511 7.511 0 0 0 5.597-5.597c.47-2.006.47-4.093 0-6.1a7.503 7.503 0 0 0-1.063-2.45'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
    <path
      d='M16.375 15.375c-.798 1.196-2.29 2-4 2-.96 0-1.853-.254-2.592-.69M15.375 11.375c.3-.598.859-1 1.5-1s1.2.402 1.5 1M6.375 11.375c.3-.598.859-1 1.5-1s1.2.402 1.5 1'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
    <path
      fillRule='evenodd'
      clipRule='evenodd'
      d='M3.867 17.808a8.255 8.255 0 0 1-.87-2.212 14.104 14.104 0 0 1 0-6.442 8.261 8.261 0 0 1 6.157-6.156 14.103 14.103 0 0 1 6.442 0 8.253 8.253 0 0 1 2.228.879l-1.11 1.107a6.76 6.76 0 0 0-1.46-.526 12.604 12.604 0 0 0-5.757 0 6.761 6.761 0 0 0-5.039 5.039 12.604 12.604 0 0 0 0 5.756c.119.508.294.992.519 1.447l-1.11 1.108Z'
      fill='currentColor'
    />
    <path
      d='m19.375 3.375-16 16'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
  </svg>
)

export default SvgSmileyNot
