import * as React from 'react'
import { SVGProps } from 'react'

const SvgTBotDots = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 1133 470'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-tbotdots'
    {...props}
  >
    <path
      d='M0 234.606C0 105.037 105.037 0 234.606 0h663.765c129.569 0 234.609 105.037 234.609 234.606 0 129.57-105.04 234.607-234.609 234.607H234.607C105.037 469.213 0 364.176 0 234.606Z'
      fill='#fff'
    />
    <circle cx={326.161} cy={234.606} r={51.499} fill='#D6D4D9' />
    <circle cx={566.489} cy={234.606} r={51.499} fill='#EDECEE' />
    <circle cx={806.817} cy={234.606} r={51.499} fill='#EDECEE' />
  </svg>
)

export default SvgTBotDots
