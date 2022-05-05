import * as React from 'react'
import { SVGProps } from 'react'

const SvgTBotDots = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 99 41'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-tbotdots'
    {...props}
  >
    <path
      d='M0 20.5C0 9.178 9.178 0 20.5 0h58C89.822 0 99 9.178 99 20.5S89.822 41 78.5 41h-58C9.178 41 0 31.822 0 20.5Z'
      fill='#fff'
    />
    <circle cx={28.5} cy={20.5} r={4.5} fill='#D6D4D9' />
    <circle cx={49.5} cy={20.5} r={4.5} fill='#EDECEE' />
    <circle cx={70.5} cy={20.5} r={4.5} fill='#EDECEE' />
  </svg>
)

export default SvgTBotDots
