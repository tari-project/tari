import * as React from 'react'
import { SVGProps } from 'react'

const SvgCalendar = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-calendar'
    {...props}
  >
    <rect
      x={3.75}
      y={4.25}
      width={16.5}
      height={16.5}
      rx={3.25}
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <path
      d='M8.82 3a.75.75 0 1 0-1.5 0h1.5Zm-1.5 2.514a.75.75 0 0 0 1.5 0h-1.5ZM16.68 3a.75.75 0 0 0-1.5 0h1.5Zm-1.5 2.514a.75.75 0 0 0 1.5 0h-1.5ZM4.14 9.029h15.72v-1.5H4.14v1.5ZM7.32 3v2.514h1.5V3h-1.5Zm7.86 0v2.514h1.5V3h-1.5Z'
      fill='currentColor'
    />
    <rect x={8} y={10.5} width={3} height={3} rx={1} fill='#716A78' />
    <rect x={8} y={15} width={3} height={3} rx={1} fill='#716A78' />
    <rect x={13} y={10.5} width={3} height={3} rx={1} fill='#716A78' />
    <rect x={13} y={15} width={3} height={3} rx={1} fill='#716A78' />
  </svg>
)

export default SvgCalendar
