import * as React from 'react'
import { SVGProps } from 'react'

const SvgQuestion = (
  props: SVGProps<SVGSVGElement> & {
    useGradient?: boolean
    testid?: 'help-icon-cmp'
  },
) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-question'
    {...props}
  >
    <path
      d='M8.95 20.647a7.511 7.511 0 0 1-5.597-5.597 13.354 13.354 0 0 1 0-6.1A7.511 7.511 0 0 1 8.95 3.353c2.006-.47 4.094-.47 6.1 0a7.511 7.511 0 0 1 5.597 5.597c.47 2.006.47 4.094 0 6.1a7.511 7.511 0 0 1-5.597 5.597c-2.006.47-4.094.47-6.1 0Z'
      stroke={
        props.useGradient ? 'url(#paint0_linear_2104_6752)' : 'currentColor'
      }
      strokeWidth={1.5}
    />
    <circle
      cx={12}
      cy={15.5}
      r={1}
      fill={
        props.useGradient ? 'url(#paint1_linear_2104_6752)' : 'currentColor'
      }
    />
    <path
      d='M10 10v-.5a2 2 0 0 1 2-2v0a2 2 0 0 1 2 2v.121c0 .563-.223 1.102-.621 1.5L12 12.5'
      stroke={
        props.useGradient ? 'url(#paint2_linear_2104_6752)' : 'currentColor'
      }
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
    <defs>
      <linearGradient
        id='paint0_linear_2104_6752'
        x1='21'
        y1='3'
        x2='-6.88286'
        y2='19.1565'
        gradientUnits='userSpaceOnUse'
      >
        <stop stopColor='#9330FF' />
        <stop offset='1' stopColor='#593A9B' />
      </linearGradient>
      <linearGradient
        id='paint1_linear_2104_6752'
        x1='13'
        y1='14.5'
        x2='9.9019'
        y2='16.2952'
        gradientUnits='userSpaceOnUse'
      >
        <stop stopColor='#9330FF' />
        <stop offset='1' stopColor='#593A9B' />
      </linearGradient>
      <linearGradient
        id='paint2_linear_2104_6752'
        x1='14'
        y1='7.5'
        x2='7.18734'
        y2='10.658'
        gradientUnits='userSpaceOnUse'
      >
        <stop stopColor='#9330FF' />
        <stop offset='1' stopColor='#593A9B' />
      </linearGradient>
    </defs>
  </svg>
)

export default SvgQuestion
